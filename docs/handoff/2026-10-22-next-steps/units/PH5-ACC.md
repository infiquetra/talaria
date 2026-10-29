# PH5-ACC: Phase 5 acceptance: a three-provider review workflow survives a kill and resumes without re-running finished agents

Status: `accepted`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order and how the result is recorded. This document holds what the people doing the work would otherwise have to guess: how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (adapted from the phase 4 acceptance unit's and run while drafting), the exact steps and pass conditions of the 10 cross-feature rows and of the operator's own check (the roadmap's phase 5 check, six rows), the operator sitting, the run order and rulings, and the design of the `phase5` smoke suite. Sections are numbered so the issue can point at them.

Handoff documents are linked relatively: [the roadmap's phase 5](../07-roadmap-and-work-units.md#phase-5--workflows), [the workflows design](../06-workflows.md), [requirement R8](../01-vision-and-requirements.md), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here). This unit copies the record, checker, evidence rules and smoke-extension pattern of [PH4-ACC](PH4-ACC.md) (itself a copy of PH4A-ACC's), changes what must change, and says so where it does. Where this document says "as PH4-ACC", read that document's section of the same name.

## 1. What this unit is for, in plain terms

Nine issues of the phase 5 capability each passed their own scenarios on their own commit: the script engine (P5-01), the journal and resume (P5-02), the Hermes binding of `agent()` (P5-03), staffing and scheduling (P5-04), the progress view (P5-05), the runner lifecycle (P5-06), worktree isolation (P5-07), the four built-in workflows (P5-08) and ultracode with the `talaria-workflows` skill (P5-09). Most of them were proved against a mock backend, a fake gateway or a recorded fixture. What no child proves is the seams on a real gateway: a real review workflow whose agents run on three providers, a runner that is killed with agents in flight, a resume that must not pay twice, and a progress view and a cost figure that must agree with what the backend actually reported.

The unit does four things.

1. It extends the smoke suite of P1-13 with `--suite phase5`: a 10-step, read-only run that drives the progress view, the ultracode status, the built-in workflow tools and the no-gateway behaviour in a real pane. The earlier six suites (phase 1, 2a, 2b, 3, 4a, 4b) each run once more as regressions.
2. It reruns every user acceptance scenario of the nine issues (52) and adds 10 scenarios that only exist when features combine, plus one row for the operator-only live tests.
3. It defines the operator's own check for phase 5 as six concrete rows (OPC-1 to OPC-6). The roadmap's check is: "`review-branch` with three reviewers on three providers; kill Talaria and the runner mid-run; resume; finished agents are not re-run; the cost shown matches the backend's usage."
4. It records the result in two markdown files, checked by a short script (about 60 lines), as phase 4 does.

The finding of `P5-00` (the gateway concurrency measurement) is a start gate, not an accepted child: its own five scenarios were run and accepted on `P5-00`. This unit uses its recommended default width as a number to test against (XF-9) and does not rerun the probes, which spend the operator's provider quota.

## 2. Facts

### 2.1 The `herdr` command line

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) and section 2.1 of [PH4A-ACC](PH4A-ACC.md) apply unchanged (Herdr 0.9.4 when they were checked; re-check with the Herdr in use and write `herdr --version` in the record header). Three matter here: `pane run` types text and Enter together; `pane read` has sources `visible`, `recent` and `recent-unwrapped`; a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. `tmux` is needed in two places only: AUTO-9 (the phase 2a suite) and XF-10 (an 80 by 24 screen).

### 2.2 Hermes and Talaria facts used (re-check at the release in `crates/hermes-contract/PINNED`)

- `prompt.submit` accepts `unattended` only together with `finite: true`, and then auto-approves the turn (`admit_finite` and `finite_turn_scope`, `gateway/session_finite.py` at Hermes v0.22.4). Nothing here sends it, and XF-5 proves the workflow runner does not.
- Runner commands, as `P5-06` defines them (REQ-7, REQ-10 to REQ-17): `talaria workflow run [--detach] [--json] [--args <json>] [--args-file <path>] [--profile <name>] [--staffing <path>] [--cwd <dir>] [--budget-tokens <n>] [--budget-usd <amount>] [--on-unknown wait|fail] [--record <path>] [--runs-dir <dir>] [--workflows-dir <dir>] <script.js>`; `resume`, `stop`, `list [--json]` and `resolve`. Exit codes of `run`: 0 completed, 1 failed, 2 refused, 3 aborted. `list --json` prints, per run: `run_id`, `name`, `profile`, `started_at_ms`, `status` (`running|parked|completed|failed|aborted|interrupted|unreadable`), `agents_finished`, `agents_total_known`, `parked`, `tokens_spent` (settled token sum or `null`), `money_spent_usd` (or `null`), `run_dir`, `warning`. A detached run writes `runner.json` (the runner's process id) in its run folder and removes it when the runner ends cleanly. The runs directory is `--runs-dir`, else `TALARIA_RUNS_DIR`, else `runs/` in the configuration folder (`TALARIA_CONFIG_DIR`, else `~/.talaria`).
- A run whose runner died without finishing shows `interrupted` in `list` (P5-06 REQ-6: not finished, not parked, no lock holder). Resume is `talaria workflow resume <run-id>`; finished agents come from the journal and are not sent again (P5-02, P5-06 UAT-3).
- Money is never invented: a run shows `cost unknown` (never `$0.00`) when receipts say the cost status is unknown (P5-05 REQ-11, P5-04 REQ for `money_text`). Whether Hermes gives a cost for the `ollama-cloud` provider is **verify at the pinned commit** (P5-04 says Hermes has no price for it).
- The staffing table is `staffing.toml` in the configuration folder, with `[tiers.<name>]` blocks of `model`, `provider` and optional `effort` (P5-04 REQ-2 and unit specification section 3). `review-branch` takes its reviewers from tiers `reviewer-1` to `reviewer-3` (P5-08 REQ-14).
- A recording made with `--record <file>` holds both directions of every gateway frame with a generation number, in the frame-log format (`docs/formats/frame-log.md`, version 3, owned by P1-04). Counting frames by method for the "finished agents are not re-run" proof uses that format's own field names; the helper `outcount` of section 3 is written once at the freeze from that document and committed to the evidence file.
- Worktree isolation (`isolation: 'worktree'`) uses the local `git` command (P5-07).

### 2.3 What the source issues provide

Each scenario is owned by one source issue; Appendix A indexes them with persona and runner. The ignored live tests are named in section 7.1. The nine source issues' final bodies are the authority for every scenario.

## 3. Freezing and building the candidate

The smoke extension (section 9) is built and merged first, on branch `p5-acc-smoke`, by a test author and a developer; it is part of the candidate. The lead freezes after the nine accepted issues, `P5-00`, `PH4-ACC`, P1-13 and the earlier acceptance units are merged and closed. Never switch the primary checkout; work in a worktree outside the repository.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p5-acc                                     # a new folder outside the repository
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p5-acc-candidate"   # for the bench runs of AUTO-1; deleted at closeout
cargo build --release --workspace --examples               # the one release build: these commands together,
cargo build --release -p talaria-cli -p talaria-bench      # run once at the freeze and never again on this candidate
cargo build --release -p talaria-cli --features workflow-mock --target-dir "$WT/target-mock"   # the mock-backed binary some source rows name (P5-06)
shasum -a 256 target/release/talaria                       # the binary every live row uses
git switch -c p5-acc-record                                # the record branch starts at the candidate commit
```

The record branch holds documentation commits only (AC-6 proves it). After a new candidate the branch is rebased onto it. `main` may move during the run: the record is compared with the candidate, never with `main`.

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

Helpers every XF and OPC row uses (the helpers of section 3 of PH4-ACC, plus four for workflows):

```bash
pane_new() { herdr pane split "$HERDR_PANE_ID" --direction right --no-focus --cwd "$1" | jq -r '.result.pane.pane_id'; }
submit()   { herdr pane send-text "$1" "$2"; sleep 0.3; herdr pane send-keys "$1" enter; }   # text, a 300 ms wait (REQ-4: Enter inside 120 ms of a paste is a line break), then Enter; every pane row uses this helper, with no extra fallback Enter
vis()      { herdr pane read "$1" --source visible; }
rec()      { herdr pane read "$1" --source recent-unwrapped --lines 3000; }
FH=~/.hermes/profiles/fixture                                                     # the fixture profile's home
plain()    { perl -pe 's/\e\[[0-9;?]*[ -\/]*[@-~]//g; s/\e\][^\a]*\a//g'; }      # strips terminal escape sequences
fleet()    { "$T" fleet list --profile fixture "$@"; }                            # PH3-ACC's helper
snap()     { find "$FH" -maxdepth 2 \( -path "$FH/sessions" -o -path "$FH/logs" \) -prune -o -print | sort | while IFS= read -r f; do rel=${f#$FH/}; if [ -L "$f" ]; then printf '%s link %s\n' "$rel" "$(readlink "$f")"; elif [ -f "$f" ]; then printf '%s %s\n' "$rel" "$(shasum -a 256 < "$f" | cut -c1-64)"; else printf '%s dir\n' "$rel"; fi; done; }   # recursive name+digest+link inventory of the protected profile top levels and skill folder; kept private, only the cmp result is evidence; an in-place edit without a listing change fails it (a negative control edits SKILL.md in place in scratch and expects a mismatch)
# workflow helpers (new in phase 5). A row sets X=$(mktemp -d) first; every run of a row uses its own folders.
wfenv()    { export TALARIA_RUNS_DIR="$X/runs" TALARIA_CONFIG_DIR="$X/cfg"; mkdir -p "$X/runs" "$X/cfg"; }   # one scratch runs folder and config folder per row
wf()       { "$T" workflow "$@"; }                                                 # the workflow command family of the candidate binary
wfjson()   { wf list --json | jq -c --arg r "$1" '.[] | select(.run_id == $r)'; }  # one run's summary object
outcount() { python3 "$X/outcount.py" "$1" "$2" "$3"; }   # defined now, not at the freeze: outcount.py (written from docs/formats/frame-log.md version 3 at build time, kept in scratch, its source pasted into evidence) counts frames the runner SENT with method $2 for agent key $3, correlating each create/submit request with its response and the journal's key-to-session map. A zero count is evidence only when the recording passes completeness: the resumed recording finished successfully, holds no dropped or unparsable (null/parseError) frame and no dropped-record marker, contains the row's positive controls (at least one create and one submit for an unfinished key), and every pre-kill finished key from the journal was looked up. The killed first recording is labelled potentially truncated; its missing frames never prove anything. Malformed and dropped-record inputs make outcount exit 2 (a negative control in evidence).
```

`snap` prints the name and digest of the top-level files of `fixture`'s home and is never pasted; only `cmp` results go into evidence. Phase 5 writes outside `snap` too (the skill install of P5-09 UAT-1 writes into the profile's skill folder; `builtins install` writes into the configuration folder, which every row points at a scratch folder with `wfenv`). So rows that claim "no change to the profile" compare full `snap` inventories before and after (names, digests and link targets, including the skill folder; the folder name is **verify at build time**: P5-09 REQ names it); a bare directory listing never proves contents unchanged. Every captured text goes through `plain` before any `grep`, anchored match, comparison or width count.

**"The composer is visible", "Back to the composer", `$BASE`.** As PH4-ACC section 3, unchanged (the last three non-blank lines of an idle read are the baseline; Esc closes a card, clears a filter, then closes the overlay, at most 4 presses; `BASE` is the `main` commit the smoke extension started from, posted as `BASE=<sha>`).

`--no-ensure` is on every launch that starts a live interactive Talaria session, so Talaria can never start a gateway. Workflow commands (`workflow run`, `resume`, `list`, `stop`, `roster`, `builtins`) take no `--no-ensure` flag, never start a gateway, and find it through the Hermes home like every other client.

## 4. The record

File `docs/acceptance/v0.7.0/phase-5.md`. Commit order (the body's order wins): first the checker tests with the stub (`test(acceptance): add the phase 5 checker tests and a stub`), second the skeleton below with every header line filled in with its real value and `phase-5-evidence.md` holding only its title line (`docs(acceptance): add the empty phase 5 record`), then the real checker (`docs(acceptance): add the phase 5 checker`), so the first run of the real checker on the skeleton is the red run: 84 problems. The checker refuses a header that still holds an angle-bracket placeholder; the values below are examples. Row order is the order below; do not add other tables.

```markdown
# Phase 5 acceptance record

Candidate: 0123456789abcdef0123456789abcdef01234567
Hermes: v0.22.4
Herdr: herdr 0.9.4
OS: macos
Date: 2026-12-21

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
| AUTO-13 | builder | NOT RUN |  |  |
| AUTO-14 | builder | NOT RUN |  |  |
| AUTO-15 | builder | NOT RUN |  |  |
| P5-01 UAT-1 | workflow-runner | NOT RUN |  |  |
| P5-01 UAT-2 | scripter | NOT RUN |  |  |
| P5-01 UAT-3 | scripter | NOT RUN |  |  |
| P5-01 UAT-4 | workflow-runner | NOT RUN |  |  |
| P5-01 UAT-5 | scripter | NOT RUN |  |  |
| P5-01 UAT-6 | workflow-runner | NOT RUN |  |  |
| P5-02 UAT-1 | workflow-runner | NOT RUN |  |  |
| P5-02 UAT-2 | workflow-runner | NOT RUN |  |  |
| P5-02 UAT-3 | fleet-lead | NOT RUN |  |  |
| P5-02 UAT-4 | workflow-runner | NOT RUN |  |  |
| P5-02 UAT-5 | developer | NOT RUN |  |  |
| P5-02 UAT-6 | workflow-runner | NOT RUN |  |  |
| P5-03 UAT-1 | workflow-runner | NOT RUN |  |  |
| P5-03 UAT-2 | workflow-runner | NOT RUN |  |  |
| P5-03 UAT-3 | workflow-runner | NOT RUN |  |  |
| P5-03 UAT-4 | fleet-lead | NOT RUN |  |  |
| P5-03 UAT-5 | workflow-runner | NOT RUN |  |  |
| P5-03 UAT-6 | workflow-runner | NOT RUN |  |  |
| P5-04 UAT-1 | configurer | NOT RUN |  |  |
| P5-04 UAT-2 | workflow-runner | NOT RUN |  |  |
| P5-04 UAT-3 | workflow-runner | NOT RUN |  |  |
| P5-04 UAT-4 | workflow-runner | NOT RUN |  |  |
| P5-04 UAT-5 | fleet-lead | NOT RUN |  |  |
| P5-04 UAT-6 | configurer | NOT RUN |  |  |
| P5-05 UAT-1 | workflow-runner | NOT RUN |  |  |
| P5-05 UAT-2 | workflow-runner | NOT RUN |  |  |
| P5-05 UAT-3 | fleet-lead | NOT RUN |  |  |
| P5-05 UAT-4 | developer | NOT RUN |  |  |
| P5-05 UAT-5 | workflow-runner | NOT RUN |  |  |
| P5-06 UAT-1 | workflow-runner | NOT RUN |  |  |
| P5-06 UAT-2 | fleet-lead | NOT RUN |  |  |
| P5-06 UAT-3 | fleet-lead | NOT RUN |  |  |
| P5-06 UAT-4 | workflow-runner | NOT RUN |  |  |
| P5-06 UAT-5 | workflow-runner | NOT RUN |  |  |
| P5-06 UAT-6 | workflow-runner | NOT RUN |  |  |
| P5-06 UAT-7 | workflow-runner | NOT RUN |  |  |
| P5-07 UAT-1 | workflow-runner | NOT RUN |  |  |
| P5-07 UAT-2 | workflow-runner | NOT RUN |  |  |
| P5-07 UAT-3 | workflow-runner | NOT RUN |  |  |
| P5-07 UAT-4 | workflow-runner | NOT RUN |  |  |
| P5-08 UAT-1 | developer | NOT RUN |  |  |
| P5-08 UAT-2 | workflow-runner | NOT RUN |  |  |
| P5-08 UAT-3 | developer | NOT RUN |  |  |
| P5-08 UAT-4 | workflow-runner | NOT RUN |  |  |
| P5-08 UAT-5 | configurer | NOT RUN |  |  |
| P5-08 UAT-6 | builder | NOT RUN |  |  |
| P5-09 UAT-1 | configurer | NOT RUN |  |  |
| P5-09 UAT-2 | scripter | NOT RUN |  |  |
| P5-09 UAT-3 | developer | NOT RUN |  |  |
| P5-09 UAT-4 | developer | NOT RUN |  |  |
| P5-09 UAT-5 | workflow-runner | NOT RUN |  |  |
| P5-09 UAT-6 | developer | NOT RUN |  |  |
| XF-1 | fleet-lead | NOT RUN |  |  |
| XF-2 | workflow-runner | NOT RUN |  |  |
| XF-3 | workflow-runner | NOT RUN |  |  |
| XF-4 | workflow-runner | NOT RUN |  |  |
| XF-5 | workflow-runner | NOT RUN |  |  |
| XF-6 | fleet-lead | NOT RUN |  |  |
| XF-7 | scripter | NOT RUN |  |  |
| XF-8 | developer | NOT RUN |  |  |
| XF-9 | workflow-runner | NOT RUN |  |  |
| XF-10 | developer | NOT RUN |  |  |
| OPC-1 | workflow-runner | NOT RUN |  |  |
| OPC-2 | workflow-runner | NOT RUN |  |  |
| OPC-3 | workflow-runner | NOT RUN |  |  |
| OPC-4 | workflow-runner | NOT RUN |  |  |
| OPC-5 | workflow-runner | NOT RUN |  |  |
| OPC-6 | workflow-runner | NOT RUN |  |  |
| OP-1 | workflow-runner | NOT RUN |  |  |
```

Header values, rules for `result`, the PASS row's `commit` and `evidence` link: as PH4-ACC section 4, with `phase-5-evidence.md#<id lower case, spaces to hyphens>` (the row `P5-06 UAT-3` links `phase-5-evidence.md#p5-06-uat-3`). BLOCKED and NOT RUN never count as a pass; there is no waiver state. Personas are those of the source scenarios (Appendix A); AUTO rows are `builder`; XF rows carry the persona named in section 7.3; OPC rows and OP-1 are `workflow-runner`. The skeleton is 94 lines (10 header lines plus 84 rows), under the 100 lines AC-12 allows.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-5-evidence.md` starts with `# Phase 5 acceptance evidence` and holds one section per row, in record order, in the shape of PH4-ACC section 5.1:

````markdown
## P5-06 UAT-3

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-12-21.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

A row run more than once keeps one `## <id>` section; each run adds its own `Candidate:` line and blocks, oldest first, and the last run is the row's result. The checker requires, for every PASS row, one section, and closed fenced blocks each holding a non-blank line and at most 60 lines; it validates **every** run's `Candidate:` line in the section (each must be 7 or more hex characters equal to the start of the candidate), so an early run on another candidate followed by a correct last run fails (checker case `evidence-names-another-candidate` covers the single-run form and a dedicated multi-run case covers this). The reviewer checks what the script cannot: the runner and date line, the `Run <k> of <n>` numbering, the three-run limit, the operator's comment URL, the row order and persona column, and that every row that is not PASS says why. An operator row holds the operator's words, the URL of the operator's comment on the issue and the read-back of each value they restored.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output. Source rows: what the source scenario's Evidence line asks for. XF and OPC rows: what sections 7.3 and 7.4 name. Smoke rows (AUTO-8 to AUTO-15): the whole output including every step line and the `SMOKE_RESULT:` line, with repeated `NOTE` lines removed (the only lines ever removed), and both tab lists. A scrubbed output over 60 lines goes in consecutive fenced blocks of at most 60 lines in the same section. Frame counts and run summaries are evidence as numbers, never as pasted recordings: a recording is attached to the operator's comment only if it passed the AC-11 secret and path scans of section 7.1 AUTO-6 applied to its scrubbed text, and is never committed. Never paste a whole configuration file, a `snap` digest or a staffing file with a real model name beyond the lines a row names.

### 5.3 Scrubbing

As PH4-ACC section 5.3 (`scrub()` replaces the home directory, login name, temporary folders and the worktree path; then read the result). New shapes in phase 5: run ids and agent keys (hex or short names: keep), run folder paths (`<runs>/<run id>`: write `<runs>`), `runner.json` process ids (keep), worktree folders created by P5-07 (write `<worktree>`), provider and model names the operator signed in for in OPC-1 (keep the provider slugs and model ids: public; drop account names, organisation names and any key tail beyond what the screen shows), and git branch names of the operator's own repository in OPC-2 (use the sample repository of P5-08 unless the operator agrees to name a branch; never a private repository name). The checker catches paths only; AC-11 also greps `--profile` values, key shapes and each runner's login name.

## 6. The checker and its tests

### 6.1 Behaviour

`check-phase-5.sh [record [evidence]]` (defaults: `phase-5.md` and `phase-5-evidence.md` next to the script). It holds the 84 row ids in one `IDS=` line. It prints one line per problem, then `ACCEPTANCE_RESULT: PASSED (<n> of 84 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 84 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. The problems are the ones of PH4-ACC section 6.1, including the repair that makes the `Candidate:` line count only once and only above the table. The script is a mechanical copy of `check-phase-4.sh` with the file names, the id list and the count changed. Rejected: one parametrised script for all phases, because it would reopen a closed gate's tool.

### 6.2 Reference implementation

Run while drafting on macOS: the 27 cases below pass (the multi-run case is listed in 6.3's introduction and follows the same helper shapes), and the skeleton record of section 4 ends `FAILED (84 problems, 0 of 84 rows PASS)` with 94 lines. Not run with GNU `awk` or `mawk`: run the cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 5 acceptance record. Usage: check-phase-5.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-5.md}
EVI=${2:-$DIR/phase-5-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-phase-5: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|AUTO-8|AUTO-9|AUTO-10|AUTO-11|AUTO-12|AUTO-13|AUTO-14|AUTO-15|P5-01 UAT-1|P5-01 UAT-2|P5-01 UAT-3|P5-01 UAT-4|P5-01 UAT-5|P5-01 UAT-6|P5-02 UAT-1|P5-02 UAT-2|P5-02 UAT-3|P5-02 UAT-4|P5-02 UAT-5|P5-02 UAT-6|P5-03 UAT-1|P5-03 UAT-2|P5-03 UAT-3|P5-03 UAT-4|P5-03 UAT-5|P5-03 UAT-6|P5-04 UAT-1|P5-04 UAT-2|P5-04 UAT-3|P5-04 UAT-4|P5-04 UAT-5|P5-04 UAT-6|P5-05 UAT-1|P5-05 UAT-2|P5-05 UAT-3|P5-05 UAT-4|P5-05 UAT-5|P5-06 UAT-1|P5-06 UAT-2|P5-06 UAT-3|P5-06 UAT-4|P5-06 UAT-5|P5-06 UAT-6|P5-06 UAT-7|P5-07 UAT-1|P5-07 UAT-2|P5-07 UAT-3|P5-07 UAT-4|P5-08 UAT-1|P5-08 UAT-2|P5-08 UAT-3|P5-08 UAT-4|P5-08 UAT-5|P5-08 UAT-6|P5-09 UAT-1|P5-09 UAT-2|P5-09 UAT-3|P5-09 UAT-4|P5-09 UAT-5|P5-09 UAT-6|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|XF-9|XF-10|OPC-1|OPC-2|OPC-3|OPC-4|OPC-5|OPC-6|OP-1'
awk -v ids="$IDS" -v evi="$EVI" -v pathbad="$PATHS" '
function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
function slug(s) { s = tolower(s); gsub(/ /, "-", s); return s }
function bad(msg) { print msg; problems++; rowbad = 1 }
function iscand(s) { return length(s) == 40 && s ~ /^[0-9a-f]+$/ }
BEGIN {
  n = split(ids, list, "|"); for (i = 1; i <= n; i++) want[list[i]] = 1
  fence = "^\140\140\140"
  while ((getline line < evi) > 0) {            # evidence: "## <id>", a Candidate: line, then fenced blocks with text
    if (line ~ fence) { infence = !infence; if (infence) { blk = 0; btext = 0 } else if (cur != "") { if (blk > longest[cur]) longest[cur] = blk; if (btext > 0) text[cur]++; else blank[cur]++ }; continue }
    if (infence) { if (cur == "") continue; blk++; if (line ~ /[^ \t]/) btext++; continue }
    if (line ~ /^## /) { cur = trim(substr(line, 4)); sect[cur] = 1; nsect[cur]++ }
    else if (cur != "" && line ~ /^Candidate: /) { split(trim(substr(line, 12)), t, /[. ]/); if ((cur in evcand) && evcand[cur] != t[1]) evbad[cur] = 1; evcand[cur] = t[1] }
  }
  unclosed = infence                              # a block counts only once its closing fence is seen
}
/^Candidate: / { if (intable || ncand++) bad("the line Candidate: must appear once, above the table"); else cand = trim(substr($0, 12)); next }
/^(Hermes|Herdr|OS|Date): / { k = $0; sub(/: .*/, "", k); hdr[k] = trim(substr($0, length(k) + 3)) }
/^\|/ {
  intable = 1; m = split($0, c, "|"); id = trim(c[2]); rowbad = 0
  if (id == "id" || id ~ /^-+$/) next
  if (!(id in want)) { bad("unexpected row: " id); next }
  if (m != 7) { bad(id ": the row needs five cells"); next }
  if (id in seen) { bad(id ": appears twice"); next }
  seen[id] = 1; result = trim(c[4]); evid = trim(c[5]); commit = trim(c[6])
  if (result !~ /^(PASS|FAIL|NOT RUN|BLOCKED \([^)]+\))$/) { bad(id ": the result must be PASS, FAIL, BLOCKED (cause) or NOT RUN, not: " result); next }
  if (result != "PASS") { bad(id ": " result); next }
  if (!iscand(cand) || length(commit) < 7 || index(cand, commit) != 1) bad(id ": commit " commit " is not the candidate")
  if (evid != "phase-5-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-5-evidence.md#" slug(id))
  if (!(id in sect) || text[id] < 1) bad(id ": the evidence file has no section with a fenced block holding text for it")
  else if (longest[id] > 60) bad(id ": a fenced block of the evidence is longer than 60 lines")
  else if (blank[id] > 0) bad(id ": a fenced block of the evidence holds no text")
  if (id in sect && (evbad[id] || !iscand(cand) || length(evcand[id]) < 7 || index(cand, evcand[id]) != 1)) bad(id ": the evidence section must name the candidate (Candidate: <7 hex>.)")
  if (!rowbad) passes++
}
END {
  if (!iscand(cand)) bad("the line Candidate: <40 hex characters> is missing or not above the table")
  for (k in hdr) if (hdr[k] ~ /[<>]/) bad(k ": the header line is still a placeholder")
  for (k in hdr) if (hdr[k] == "") bad(k ": the header line has no value")
  if (unclosed) bad("the evidence file ends inside an unclosed fenced block")
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

27 cases: the 26 pasted cases of PH4-ACC section 6.3 with three ids renamed, plus a 27th case the test author writes from this description (an evidence section holding an early run naming another candidate followed by a correct last run, expecting `the evidence section must name the candidate`) (`P5-09 UAT-6` for not-run, `P5-06 UAT-2` for the wrong commit, `P5-08 UAT-6` for the missing section) and `84` for `67`. The test script reads the id list out of the checker, so the two cannot drift; `all-pass` expects the text `84 of 84`, so a dropped id fails it. The stub the test author commits first is

```sh
#!/bin/sh
# stub: the real checker replaces this file
IDS='<the same IDS line as in the reference checker>'
echo "ACCEPTANCE_RESULT: FAILED (stub)"; exit 1
```

so every case fails on its expectation. Independence means the test author is not the developer who writes the checker.

````sh
#!/bin/sh
# Tests for check-phase-5.sh. Run: sh docs/acceptance/v0.7.0/check-phase-5.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-5.sh"
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
      echo "| $id | developer | $res | phase-5-evidence.md#$slug | $com |"
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
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (84 of 84 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P5-09 UAT-6" "NOT RUN";               expect not-run-fails 1 "P5-09 UAT-6: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P5-06 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-1' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-1: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P5-08 UAT-6";               expect no-evidence-section 1 "P5-08 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-5-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
{ cat "$T/ev.md"; printf 'see /%s/someone/project\n' Users; } > "$T/ev3.md"; expect local-path 1 "a local path appears" "$T/good.md" "$T/ev3.md"
expect missing-file 2 "not found" "$T/nope.md" "$T/ev.md"
make_evidence "$T/ev4.md" "AUTO-2"; section AUTO-2 "" >> "$T/ev4.md"; expect empty-evidence-block 1 "AUTO-2: the evidence file has no section" "$T/good.md" "$T/ev4.md"
make_evidence "$T/ev5.md" "AUTO-2"; section AUTO-2 "$(seq 1 61)" >> "$T/ev5.md"; expect long-evidence-block 1 "AUTO-2: a fenced block of the evidence is longer than 60 lines" "$T/good.md" "$T/ev5.md"
make_record "$T/k.md" "AUTO-3" PASS 012345;            expect short-commit 1 "AUTO-3: commit 012345 is not the candidate" "$T/k.md" "$T/ev.md"
{ cat "$T/good.md"; printf 'see /%s/someone/project\n' tmp; } > "$T/l.md"; expect local-path-in-record 1 "a local path appears" "$T/l.md" "$T/ev.md"
sed 's/^Hermes: .*/Hermes: <pinned release tag>/' "$T/good.md" > "$T/m.md"; expect placeholder-header 1 "Hermes: the header line is still a placeholder" "$T/m.md" "$T/ev.md"
sed 's/^Candidate: 0123456\./Candidate: deadbee./' "$T/ev.md" > "$T/ev6.md"; expect evidence-names-another-candidate 1 "AUTO-1: the evidence section must name the candidate" "$T/good.md" "$T/ev6.md"
make_evidence "$T/ev7.md" none; section AUTO-2 ok >> "$T/ev7.md"; expect duplicate-section 1 "AUTO-2: the evidence file has 2 sections for it" "$T/good.md" "$T/ev7.md"
sed 's/^Herdr: .*/Herdr: /' "$T/good.md" > "$T/n.md"; expect empty-header-value 1 "Herdr: the header line has no value" "$T/n.md" "$T/ev.md"
sed '$d' "$T/ev.md" | sed '$d' > "$T/ev8.md"; expect unclosed-fence 1 "the evidence file ends inside an unclosed fenced block" "$T/good.md" "$T/ev8.md"
{ grep -v '^Candidate: ' "$T/good.md"; echo "Candidate: $C"; } | sed 's/| [0-9a-f]\{12\} |$/| deadbeefdead |/' > "$T/o.md"; sed "s/^Candidate: $S\./Candidate: deadbee./" "$T/ev.md" > "$T/ev9.md"; expect late-candidate-header 1 "the line Candidate: must appear once, above the table" "$T/o.md" "$T/ev9.md"
make_evidence "$T/ev10.md" "AUTO-2"; { section AUTO-2 ok; printf '```text\n \n```\n\n'; } >> "$T/ev10.md"; expect empty-block-after-text 1 "AUTO-2: a fenced block of the evidence holds no text" "$T/good.md" "$T/ev10.md"
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-5.md" && cp "$T/ev.md" "$T/d/phase-5-evidence.md"
out=$(sh "$T/d/check-phase-5.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record, taken at the commit that adds the real checker while the committed record is still the skeleton of section 4: `sh check-phase-5.sh` ends `ACCEPTANCE_RESULT: FAILED (84 problems, 0 of 84 rows PASS)` and exits 1 (checked while drafting).

## 7. The scenario catalogue

### 7.1 AUTO rows (persona builder; agent tester)

Run at the worktree root. Each row's evidence is the command and the last 15 lines. `E=docs/acceptance/v0.7.0`.

| Row | What | Command and pass condition |
|---|---|---|
| AUTO-1 | Project check and continuous integration on the candidate | `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace` exits 0; `gh run list -R infiquetra/talaria --commit "$CANDIDATE" --json conclusion,workflowName` shows every required workflow `success` (poll every 10 s, at most 5 minutes, for a run on the candidate; an older run does not count); `bench` run on the candidate (as PH4-ACC AUTO-1; none run: BLOCKED (environment)); the binary's `shasum -a 256` is recorded |
| AUTO-2 | Wire and replay suites | `cargo test -p talaria-workflow --features mock,fake-gateway` and `cargo test -p talaria-workflow --test hermes_frames --test hermes_validate --test hermes_policy` exit 0, plus the earlier phases' replay suites by `cargo test --workspace --test '*replay*'` (exit 0) |
| AUTO-3 | Workflow unit and property suites | `PROPTEST_CASES=1024 cargo test -p talaria-workflow --features mock,fake-gateway --test journal_crash_points`; `cargo test -p talaria-workflow --test progress_tree --test progress_control --test progress_live --test runner_runs --test runner_resolver --test runner_execute`; `TALARIA_REQUIRE_GIT=1 cargo test -p talaria-workflow --test worktree_isolation`; `cargo test -p talaria-workflow --features mock --test 'builtins_*'`; `cargo test -p talaria-workflow --test skill_files --test skill_install --test roster_envelope`; `cargo test -p talaria-core --test ultracode_route`; `cargo test -p talaria-app --test ultracode_app`; `cargo test -p talaria-tui --test ultracode_status --test workflow_view`; `cargo test -p talaria-cli --features workflow-mock --test workflow_cli`; `cargo test -p talaria-cli --test workflow_builtins_cli --test workflow_skill_cli`: each exits 0 with no `0 passed` |
| AUTO-4 | Budgets | `cargo test -p talaria-workflow --release --test journal_bench -- --ignored --nocapture` and `cargo test -p talaria-workflow --release --test progress_tree -- --ignored ten_thousand` exit 0 and print the figures the source issues name (the figures are copied into evidence, not restated here) |
| AUTO-5 | Live read-only suites | each with `TALARIA_LIVE_REQUIRE=1` or the variable its source names, exit status saved before output is shown and equal to 0, and no `skipped:` line: `cargo test -p talaria-workflow --features hermes-live --test hermes_live -- --ignored --nocapture --skip live_dangerous_command_denied_without_waiting` (P5-03; test 35 `live_dangerous_command_denied_without_waiting` is explicitly skipped here, needs `TALARIA_LIVE_APPROVALS_CONFIRMED=1` and belongs only to OP-1; the three remaining ignored tests must pass, not skip); `TALARIA_REQUIRE_LIVE=1 cargo test -p talaria-workflow --features hermes-live --test roster_live -- --ignored --nocapture` (P5-04); `TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test workflow_progress_live -- --ignored --nocapture` (P5-05); `cargo test -p talaria-cli --test workflow_live -- --ignored --nocapture` (P5-06); `cargo test -p talaria-workflow --test worktree_isolation live_worktree_agent_edits_only_the_worktree -- --ignored --exact --nocapture` (P5-07); `cargo test -p talaria-cli --test workflow_builtins_live -- --ignored --nocapture` (P5-08); `cargo test -p talaria-workflow --test live_ultracode -- --ignored --nocapture` (P5-09); `TALARIA_LIVE_REQUIRE=1 TALARIA_RECORD_FIXTURE=0 cargo test -p talaria-workflow --features mock,fake-gateway --test journal_live -- --ignored --nocapture --test-threads 1 --skip live_unknown_after_restart` (P5-02; record-fixture mode is left off so no file changes) |
| AUTO-6 | Hygiene scans | `git grep -n unattended -- crates/talaria-workflow/src crates/talaria-workflow/workflows crates/talaria-workflow/skill` prints no line that sets the key (the comment lines a source names are listed in evidence); `cargo tree -p talaria-workflow -e normal` shows none of `ratatui`, `crossterm`, `talaria-tui` or `hermes-client`; `hermes-contract` may appear only through the P5-05 progress edge (P5-05 REQ adds `talaria-workflow` on `hermes-contract`, superseding P5-01's original whole-crate scan for the final graph), and the engine isolation is checked where P5-01 owns it: no file under the engine modules named by P5-01's specification imports `hermes_contract`, `hermes_client`, `ratatui`, `crossterm` or `talaria_tui` (grep in evidence; a progress-module import is not an engine import). The AC-11 secret and path scans run over the acceptance folder in stage 1; the same scan over XF-1's committed evidence section (its scrubbed text and counted numbers in `phase-5-evidence.md`) is AUTO-6's second half, executed immediately after XF-1 in stage 2 and recorded under AUTO-6, never over the raw run folder, whose launch data legitimately holds absolute working directories; AUTO-6 is not PASS until both halves pass, so REQ-14's stage order completes it in stage 2; three positive controls (a planted `sk-` shaped text, a planted path and a planted non-fixture profile name in a temporary file each make the scan report a hit), recorded by label and count only |
| AUTO-7 | The smoke tool's own tests | `cargo test -p smoke` exits 0; `cargo test -p smoke -- --list --ignored 2>/dev/null` differs from the list at `$BASE` by exactly one added ignored test (`live_phase5_smoke_in_herdr`) |
| AUTO-8 to AUTO-13 | The earlier suites, once each | `TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite <name> --talaria "$T"` for phase 1 (6 `PASS` lines, n under 300), phase 2a (21; 1800), phase 2b (15; 1500), phase 3 (12; 1200), phase 4a (17; 900), phase 4b (14; 900); `SMOKE_RESULT: PASSED (elapsed <n> s)`; exit 0; no `talaria-smoke-` tab left. The counts are those of PH4-ACC at its freeze: re-count them against `tools/smoke/README.md` at this freeze. A skipped step is BLOCKED (environment) (ruling 1, section 8.1) |
| AUTO-14, AUTO-15 | Phase 5 smoke suite, twice | `TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase5 --talaria "$T" > "$X/run.txt" 2>&1; echo "exit=$?"`: 10 `PASS` lines, `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 600, exit 0, no `talaria-smoke-` tab left, and `snap`, the skill folder listing and the runs-folder listing unchanged (`cmp` prints 0 each); the suite cannot skip, so a skip is a FAIL |

### 7.2 Source rows (52 scenarios)

Every user acceptance scenario of the nine issues is rerun on the candidate **as the source currently writes it** (the issue body on the day of the freeze, not the text quoted anywhere else). Only four changes are allowed: `$T` (the absolute release binary) for `talaria`; `--no-ensure` on live interactive launches; `--talaria "$T"` on `smoke run`; the pane split form of section 3 where Herdr rejects the scenario's. The mock-backed rows use the binary built with `--features workflow-mock` (section 3) or the `cargo run ... --example` forms as written. A demo or test that is absent is BLOCKED (not built).

Four closed exceptions to verbatim rerun (the only ones; anything else is a stop condition): (a) `P5-02 UAT-5` records in place, then restores: run its source command with `TALARIA_RECORD_FIXTURE=1` exactly as written (it overwrites `fixtures/frames/p5-02-idempotency.jsonl`), validate that new recording with `TALARIA_REQUIRE_FIXTURE=1 cargo test -p talaria-workflow --features mock,fake-gateway fake_gateway_rules`, then restore with `git checkout -- fixtures/frames/p5-02-idempotency.jsonl`. The row PASS requires the validation, a clean `git status --porcelain -- fixtures/` afterwards, and no fixture committed after the freeze. (b) `P5-05 UAT-1` and `UAT-5`, whose sources say the gateway is stopped, run with `HERMES_HOME` pointed at an empty folder (no gateway reachable) instead of stopping the real gateway; no operator stop or restart is scheduled for them. (c) The P5-09 skill order is: agent runs `P5-09 UAT-4` (skill absent) first, the operator then installs the real `fixture` skill, then the agent runs `UAT-3` and `UAT-6`; AUTO-5's `live_ultracode` runs after that install. (d) A source scenario may rename its own local variables (P5-01's `T` is its output folder) and each row runs in a fresh shell that reloads `../acc.env` first, so the shared binary variable is never overwritten.

Runner assignment (Appendix A lists all 52):

- **Operator rows (2):** `P5-04 UAT-5` (a rate-limited provider holds its agent until the reset; the operator needs a provider that is actually limited, or the reset time the source scenario names) and `P5-09 UAT-5` (a substantive task becomes a workflow; script quality depends on the lead model and the run spends tokens).
- **Paired row (1):** `P5-06 UAT-5` (answer a parked call with `resolve`): the agent tester drives it and the operator restarts the gateway when asked. It is one of the 11 items of the operator sitting (section 7.5) because only the operator may restart the gateway.
- **Agent rows (49):** all others. `P5-03 UAT-3` needs the operator's approvals precondition (set before the run, in the Dependencies list of the issue); if it is absent the row is BLOCKED (environment).

Rulings that apply here (section 8.1 has the full list):

1. A source row that needs a live model is run on `minimax-m3.1-flash` on `ollama-cloud` with the `fixture` profile. A run that stops because the model, backend or approval policy stopped it is BLOCKED, never PASS (P5-08's shared rule).
2. `P5-02 UAT-1` and `P5-08 UAT-6` use the mock examples built at the freeze; they are proved on the candidate commit, not on a fixture from an earlier commit.
3. The counting rule of `P5-06 UAT-3` is the proof of "not re-run": the number of session-create and prompt-submit frames the resumed runner sent for a finished key is 0. A session count alone is never evidence.

### 7.3 Cross-feature rows XF-1 to XF-10

Each agent row has its own `mktemp -d` folder `X`, calls `wfenv`, and starts with `echo READ-CHECK` and a read that shows it. Staffing for live rows: a file `$X/staffing.toml` with the tiers `survey`, `builder`, `judge`, `reviewer-1`, `reviewer-2`, `reviewer-3` all set to `model = "minimax-m3.1-flash"` and `provider = "ollama-cloud"` (the same content as P5-08's `staffing-fixture.toml`, which the row copies from the candidate checkout). The sample repositories are made with P5-08's `make-sample-repo.sh` (`mk review`, `mk failing`).

**XF-1 (fleet-lead). The progress tree and the list agree.** Start a detached run of a three-agent script of P5-06 UAT-2 (`wf run --detach --profile fixture three.js`). Open Talaria with `$T $F` in a second pane with the same `TALARIA_RUNS_DIR`, type `/workflows`, and read: the run is listed, with its phases and three agents. Pass: while the run is `running` the tree shows the run and agents (counts of finished agents equal `agents_finished` in `wfjson`, within the one refresh the view needs: read both within 5 seconds); after the run finishes the tree and `wfjson` both show 3 of 3 and `completed`; the tree opened **after** the run finished (a new Talaria launch with `HERMES_HOME` at an empty folder, so no gateway is reachable) shows the same totals from the journal alone; that empty-home launch is the proof that no gateway call was needed. Evidence: both reads and the `wfjson` line, with the totals compared.

**XF-2 (workflow-runner). Width, resume and a staffing change together.** Staffing is the section 7.3 tier set plus a `probe-unused` tier and `[limits] max_in_flight = 3`; run P5-04's twelve-agent script (`fan_out_12.js`) detached with `--record "$X/rec1.jsonl"`; when 4 agents have finished, `kill -9` the runner (pid from `runner.json`); edit `staffing.toml` changing only the `effort` of a tier that no finished or in-flight call uses (the file defines a `probe-unused` tier that the twelve-agent script never references; the used tiers keep their resolved effort, so finished identities are unchanged), passing that same staffing file to both run and resume; `wf resume <run-id> --record "$X/rec2.jsonl"`. Pass: the concurrency peak is computed across both recordings as one timeline seeded from rec1's still-active sessions and the journal (never reset to zero at resume) and is at most 3; the resume prints one staffing-drift line naming `probe-unused` and still finishes; `outcount` for create and submit frames of the 4 finished keys in `rec2.jsonl` is 0 under section 3's completeness rule; the run ends `completed` with 12 of 12. Changed-used-tier rerun behaviour stays in P5-04 UAT-6 and is not claimed here. Evidence: the two maxima, the drift line, the per-key counts, the final `wfjson` line.

**XF-3 (workflow-runner). Kill and resume with worktrees.** In `mk review`'s repository, a script of two agents with `isolation: 'worktree'` (P5-07 UAT-1's script), run detached with `--record`; before the kill, record `git worktree list` and a name-and-digest inventory of both worktrees; after the first agent finished, `kill -9` the runner; resume from a different folder (`cd "$X"` then `wf resume <run-id>`). Pass: the finished agent replays from its cache (separate assertion: folder unchanged, zero new frames); an **unfinished** call whose worktree already exists is reattached on resume from a different folder: its recorded launch working directory equals that worktree, edits it made before the kill are still present, and it finishes there. The second agent finishes in its own worktree, `outcount` of create and submit frames for the finished key in the named resume recording `rec2` (the resume command carries `--record "$X/rec2.jsonl"`) is 0 under section 3's completeness rule, and `git status --porcelain` in the main checkout is empty. Evidence: the two worktree listings, the counts, the status.

**XF-4 (workflow-runner). The cost figures agree.** Run `review-branch` (`mk review`, fixture staffing) in the foreground with `--record "$X/rec.jsonl" --json`. Pass: the run summary's `tokens` (final JSON line `spend.tokens`), `wfjson`'s `tokens_spent` and the sum of settled per-admission usage totals counted once per admission in `rec.jsonl` are three equal exact integers (if the total is 1,000 or below the row still compares them exactly and the view check uses the exact form; the run is chosen so the expected total exceeds 1,000, and a total at or below it is recorded, not failed for that reason alone); the progress view shows that sum rendered by P5-05's format (under 1,000 exact, else one-decimal `k`, so 1,050 shows `1.1k`), never compared as a raw integer. Money follows P5-04 section 8.2: per session keep the last cumulative `session_cost`, add only deltas, treat a lower reading as a reset (add the new reading), price unpriced turns from the frozen price book, and compare typed amounts plus known/unknown status before checking each surface's own format (progress `cost unknown` or `$<x.xx>`, list JSON `null`, human `-`); a genuine known zero and normal rounding of a small known charge pass, and zero is rejected only when it falsely stands for unknown cost. The control readings $0.01 then $0.02 for one session total $0.02. After a kill-and-resume, distinct admissions are counted once across both recordings. Evidence: the integers (never the recording), the money text, the screen line.

**XF-5 (workflow-runner). The runner never sends `unattended` (negative).** Run, in `mk review` with the fixture staffing of section 7.3: `wf run review-branch --profile fixture --staffing "$X/staffing.toml" --record "$X/builtin.jsonl"`; P5-03 UAT-3's dangerous-command script the same way with `--record "$X/danger.jsonl"`; and one ultracode-routed prompt (`/ultracode on`, a short prompt, in a recorded Talaria session: `$T $F --record "$X/ultra.jsonl"`). Pass: each recording first proves its path ran (the built-in and dangerous recordings hold prompt-submit frames for their agents; the ultracode recording holds a prompt-submit for the routed ordinary lead turn — XF-5 tests that routed turn only, never a generated workflow, whose per-run operator approval belongs to XF-8), and only then the count of recorded outbound frames containing `unattended` or `yolo` is 0 in all three; the dangerous-command run ends with the command denied and the run not waiting (P5-03 UAT-3's observable). A recording with no submit frame fails, never passes on a zero count. Evidence: the three counts and the denial line.

**XF-6 (fleet-lead). Every way of stopping ends with no working session.** Three runs of a long script (about 60 seconds): (a) foreground, Ctrl+C after two agents started (P5-01 UAT-6's key); (b) detached, `wf stop <run-id>`; (c) detached, opened in `/workflows` and stopped from the tree (the stop key the source P5-05 UAT-3 uses). Pass: after each, `wfjson` shows `aborted` (exit code 3 for (a)); the no-working proof reads `journal.jsonl`'s `turn_settled` records per admission (the journal reader of P5-02; no other receipt tool is needed) and requires every targeted admission to hold a terminal receipt there, retained per identity as evidence, with `fleet` reads (twice, 10 seconds apart) as supplementary evidence only, because workflow tool sessions need not appear in the human fleet list; `stop` printed `stopped <run-id>` only after receipts were terminal (P5-06 UAT-4). Evidence: three `wfjson` lines, the exit codes and the fleet reads.

**XF-7 (scripter). The whole command family without a gateway (negative).** With `HERMES_HOME` pointed at an empty folder and `TALARIA_RUNS_DIR` at a folder holding one completed run, run in order: `wf list` (prints the run; exit 0); `wf resume <completed-id>` (replays the stored result with no gateway call; exit 0); `wf run one.js` (one plain gateway-not-found line; exit 1; no new run folder); `wf builtins list` (exit 0); `wf builtins check review-branch --staffing <file>` (exit 0 for a complete file, exit 2 for one without the reviewer tiers, as P5-08 UAT-5); `wf roster --profile fixture --json` (fails clearly, as P5-09 UAT-2 writes). Then drive P5-05 UAT-5 as that scenario writes it (a corrupt journal and a missing gateway) and read the progress view's banner `journal only — live state and controls unavailable`. Pass: each result above, and no run folder or file is created by the failing `run`. Evidence: the seven exit codes and lines, and the banner read.

**XF-8 (developer). Ultracode, the runs folder and the tree.** *Operator row.* With the skill installed in `fixture` (P5-09 UAT-1), in a Talaria session `/ultracode on`, then a short substantive task from P5-09 UAT-5 (the operator's own wording). Pass: the lead session writes and starts a workflow; `wf list` shows a new run with the profile `fixture`; `/workflows` shows the same run; the operator stops it from the tree; `wfjson` ends `aborted` and `fleet` shows no session of the run working within 40 seconds; the keyword form (`ultracode` in a prompt with the toggle off) starts one run without turning the toggle on (P5-09 UAT-6's observable). If the lead model does not produce a runnable script, the row is BLOCKED (environment: lead model), never PASS.

**XF-9 (workflow-runner). The width never exceeds the measured default.** Read the finding of `P5-00` (merged): its `recommendations.max_in_flight` is `W`. With a staffing file that sets no `limits.max_in_flight`, run the twelve-agent script (`fan_out_12.js`) with `--record`. Pass: the highest number of sessions created but not terminal at any moment in the recording is at most `W`; if the finding says "no healthy width" (`null`), the row is BLOCKED (environment: no healthy width), reported on the issue and on `P5-04`. Evidence: `W` with the finding's cited run id, and the measured maximum.

**XF-10 (developer). Cost of the view.** Workload (no script-level park exists and no gateway restart is used): a chain of ten-agent fan-out runs on the free model — launch the next run as soon as the previous one completes, so at every sample at least one run is active (evidenced by that run's `wfjson` reading `running`); after the last sample the current run is ended with `wf stop <run-id>` and its final `aborted` state is kept as evidence (no `resolve` release is involved). Idle baseline: sample the same Talaria process with the view open and no run for 30 seconds first. Sample `ps -o rss=,time= -p <pid>` at load start and every 30 seconds for two minutes. Formulas: memory growth is the last load sample minus the first load sample; central processing unit (CPU) cost is the load interval's CPU rate minus the idle interval's CPU rate. Also open `/workflows` in an 80 by 24 tmux session. Pass (limits carried from PH4-ACC XF-8; the operator may change them on the parent before the freeze): resident memory 40 MiB at the start and 100 MiB after, growth 4096 KiB over the two minutes, CPU 1 s per two minutes beyond the idle baseline; every line of the 80 by 24 view fits (no wrapped agent row); the idle composer returns when the view is closed. Evidence: the samples as numbers, the screen read.

### 7.4 The operator's own check: OPC-1 to OPC-6

The roadmap's check is "`review-branch` with three reviewers on three providers; kill Talaria and the runner mid-run; resume; finished agents are not re-run; the cost shown matches the backend's usage". Six rows, in this order, in one sitting, on `fixture` with the operator's own provider keys already in the profile (the operator does not paste keys here: this check uses providers that are already signed in). The sitting first creates one scratch folder (`X=$(mktemp -d)`) and calls `wfenv` in it, so the OPC runs live under that folder like XF runs and OPC-6's deletion removes them. The operator uses a sample repository from `mk review` unless they choose to name a branch of their own (section 5.3).

**OPC-1. The staffing is three providers.** Write `$X/staffing.toml` with `reviewer-1`, `reviewer-2`, `reviewer-3` on three **different** signed-in providers (for example `ollama-cloud`, plus two the operator has: slugs and models are the operator's), and `survey`, `builder`, `judge` tiers on any of them. Run `wf roster --profile fixture` and `wf builtins check review-branch --staffing "$X/staffing.toml"`. Pass: the roster lists the three providers as authenticated; `check` prints `ok` and exits 0; the cost estimate line is a range or `unknown`, never `$0.00`; the three reviewer providers are pairwise different (a `sort -u | wc -l` of the three slugs prints 3).

**OPC-2. Start the run and see three providers.** `cd` into the sample repository, `wf run review-branch --detach --profile fixture --staffing "$X/staffing.toml" --record "$X/rec1.jsonl"`, then open `/workflows` in Talaria. Pass: the tree shows the collector, three `review:` agents, then verifiers; each reviewer row names a different provider (read from the tree's agent rows and from `<run folder>/manifest.json`); `wfjson` says `running`.

**OPC-3. Kill both.** When at least one agent has finished and at least one is still running (read `wfjson`: `agents_finished` at least 1 and the status `running`), write down the finished agent keys (from the tree where it renders them, else from the run's `journal.jsonl` call keys), then kill Talaria (the pane's process) and the runner (`kill -9` the pid in `runner.json`). Pass: `wfjson` shows `interrupted` within 10 seconds; the run folder still holds `journal.jsonl`; the finished keys are listed.

**OPC-4. Resume.** `wf resume <run-id> --record "$X/rec2.jsonl"` from a different folder. Pass: the command ends with outcome `completed` (exit 0); the final report has findings in the `review-branch` shape (P5-08 REQ-14: confirmed, uncertain, refuted); for every key listed at OPC-3, `outcount` of session-create and prompt-submit frames in `rec2.jsonl` is 0; the sum of agents in `rec1.jsonl` and `rec2.jsonl` creates one session per key, no more (the distinct-session count equals the agent count of the run, taking unfinished agents once).

**OPC-5. The cost matches the backend.** Compare three numbers per provider and in total: the tokens and money the progress view and `wfjson` show; the sum of the settled usage the gateway reported in `rec1.jsonl` plus `rec2.jsonl` (each admission once; the same counting rule as XF-4); and, where the operator can open it, the provider's own usage page for the time of the run (optional; not a gate). Pass: the view's and `wfjson`'s token totals equal the recording's exact admission-once sum, with the view compared through P5-05's rendering (1,050 shows `1.1k`); money is folded per session from cumulative `session_cost` readings across both recordings (deltas only, a lower reading is a reset, unpriced turns from the frozen price book), so $0.01 then $0.02 totals $0.02, and each surface is then checked in its owner's format; a genuine known zero passes and only a zero standing for unknown cost fails; resume did not double-count finished agents.

**OPC-6. Clean up and read back.** In this order, while the journal still exists: read the run's `journal.jsonl` `turn_settled` records per admission and require each targeted admission's receipt terminal there (fleet list is supplementary only); remove any leftover worktree through P5-07's owner `remove(path, force)` as invoked by that unit's worktree test/example harness (a dirty one only with `force: true`, never an unnamed command-line flag; `review-branch` itself creates worktrees only for its file agents, so when `git worktree list` shows none this step is vacuous and recorded as such); compare `snap` with the sitting's first `snap` (the comparison status is printed, see UAT-3's rule); preserve the evidence numbers. Only then delete `$X` and close the panes, and confirm `wf list` shows no run still `running` (an empty list after deletion proves nothing by itself). Remove nothing in Hermes configuration. Pass: each of these, in this order.

### 7.5 The operator sitting (11 items) and OP-1

One sitting, one comment per item on the issue, then one comment with a line reading exactly `PHASE 5 OPERATOR ROWS: PASSED (candidate <7 hex>)`; an agent copies each into the evidence with its URL. The order: OPC-1 to OPC-6; `P5-04 UAT-5`; `P5-06 UAT-5` (the agent tester drives it and the operator restarts the gateway when asked); `P5-09 UAT-5`; `XF-8`; `OP-1`.

**OP-1. The operator-only live tests.** Run, with the variables the sources name and the operator's confirmation of each precondition: `TALARIA_OPERATOR_RESTART=1 cargo test -p talaria-workflow --features mock,fake-gateway --test journal_live live_unknown_after_restart -- --ignored --nocapture` (P5-02; the operator restarts the gateway when the test asks) and `TALARIA_LIVE_APPROVALS_CONFIRMED=1 cargo test -p talaria-workflow --features hermes-live --test hermes_live live_dangerous_command_denied_without_waiting -- --ignored --exact --nocapture` (P5-03 test 35 alone; run only after the operator checks that `approvals.mode` is `manual`; the restart row also sets `TALARIA_LIVE_REQUIRE=1`). Pass: each selects exactly one test, exits 0, prints its `JOURNAL_LIVE_RESULT: PASSED` marker where the source defines one, and any `JOURNAL_LIVE_RESULT: SKIPPED` line fails the row regardless of exit code; no `skipped:` line appears.

## 8. Run order, failures and a new candidate

Stage 0 (agents): build and merge the smoke extension, test first. Freeze (section 3). Stage 1 (agent): AUTO-1 to AUTO-7, then AUTO-8 to AUTO-15 back to back. Stage 2 (agent): the 49 agent source rows (with section 7.2's four exceptions), then XF-1 to XF-7, XF-9 and XF-10, one live scenario at a time, then this issue's UAT-4 negative smoke row in its own scratch folder (separate from AUTO-14 and AUTO-15 and from their two-run claim). Stage 3 (operator): the 11 items. Stage 4: record, checker, pull request, merge.

### 8.1 Rulings that settle questions a builder asked

1. An earlier suite's skipped step, `PASSED WITH SKIPS` or `FAIL <step> (skipped: ...)` is `BLOCKED (environment: <step> skipped)`; any other failure is FAIL. The phase 5 suite cannot skip.
2. At most three runs per row per candidate. BLOCKED may be rerun once the cause is fixed; FAIL only when it is not a Talaria defect and after `rerun approved: <id>: <cause>` on the issue. The last run counts.
3. A Talaria defect stays FAIL until a new candidate passes: the lead files a defect issue through Mission Control for the source's owner, freezes a new candidate and every row reruns.
4. A scenario dropped before the freeze is removed from this issue, the checker's list and every count in one change. After the freeze no row is dropped or waived.
5. A free-model rate limit or a provider outage is BLOCKED (environment), never a pass and never a Talaria defect unless the same run shows Talaria mishandling it.
6. The kill in OPC-3 and the XF rows is `kill -9` of the runner's process; a graceful stop is not a kill and does not count.
7. If the operator has only two signed-in providers, OPC-1 cannot pass; the row is BLOCKED (no access) and acceptance waits (section 10.2, question 1).
8. At most 6 hours of a free-model quota is spent on one candidate; if the quota runs out, the remaining rows are BLOCKED (environment) and rerun after the reset.

## 9. The smoke suite extension (`phase5`)

### 9.1 Why a seventh suite, and what it is not

Phases 2 to 4 each added a suite to `tools/smoke` so the binary is driven through a real Herdr pane, not only through tests. Phase 5 does the same for what is safe to run without spending model quota or changing the profile. It is **not** a workflow run: starting a workflow spends tokens and is covered by the source rows and the OPC rows. It is black-box: it drives the built binary through `herdr`, links no `hermes-*` or `talaria-*` crate, and reads only the screen, command output and its scratch folder. It writes only inside its own scratch folder (`TALARIA_RUNS_DIR` and `TALARIA_CONFIG_DIR` point there), never into the profile.

### 9.2 Shape of the code (proposed names; report any change)

`Phase5` joins the `Suite` enum and its `FromStr` in `tools/smoke/src/suite.rs` (default stays `phase1`; the unknown-suite text an earlier test pins stays). `run_smoke_5(term, clock, cfg, cfg5, stop)` in `tools/smoke/src/suite5.rs`; texts in `wording5.rs`; screen helpers in `screen5.rs`; steps in `steps5.rs`. `Phase5Config` is frozen here as `pub struct Phase5Config { pub profile: String, pub model: String, pub provider: String }` with defaults `fixture`, `minimax-m3.1-flash`, `ollama-cloud` (preflight rejects any other values). Screen helpers are exactly the earlier suites' helper shapes applied to section 9.5's texts: one predicate per wording constant, each taking the plain-stripped pane text and returning `bool`; their names are the constant names in lower case, listed in the test matrix of section 9.9. Preflight (exit 2, nothing created, no `SMOKE_RESULT`): `jq` missing, or `--profile`, `--model` or `--provider` other than `fixture`, `minimax-m3.1-flash`, `ollama-cloud`. Earlier entry points are unchanged. Default `--deadline-secs` for `phase5` is 600, above the step limits' sum of 410 s (a proposal; the operator may reject it before the freeze, section 10.2).

### 9.3 Flow, step by step

Ten steps in this order; a failing step ends the run; no step may SKIP. Each screen step starts and ends idle (no overlay, empty composer, no turn): it sends its keys, reads, then Esc teardown (a 400 ms wait and a read, until the title is gone, at most 3). Command steps run in a shell pane at the scratch folder.

| # | Step | Limit (s) | Passes when |
|---|---|---|---|
| 1 | startup | 30 | as P1-13: the composer is visible |
| 2 | plain-answer | 60 | a reply carrying token A |
| 3 | workflows-empty | 30 | `/workflows` opens the progress view on an empty runs folder and shows `no runs yet — talaria workflow run starts one`; Esc returns to the idle composer |
| 4 | ultracode-status | 30 | `/ultracode status` shows the status text of P5-09 REQ-22 (off at start) and no turn starts; the toggle stays off |
| 5 | quit | 20 | `/quit` ends Talaria; token A survives in the pane text |
| 6 | resume | 90 | Talaria is relaunched in a split second pane (as P1-13's resume does; the first pane keeps its shell and scrollback), shows A once or more, then a fresh token C answers; the step ends by submitting `/quit` in that pane, waiting for its successful exit and positively reading its shell prompt, so no later command step types into Talaria |
| 7 | builtins-cli | 40 | with the scratch configuration: `talaria workflow builtins install` twice prints `created folder <dir>` and four `created` lines, then four `unchanged` lines (P5-08 UAT-1's exact predicate); `builtins list` shows the four names and `installed`; `builtins check review-branch --staffing <missing-tiers file>` exits 2 and prints `missing tiers:` (P5-08 UAT-1 and UAT-5) |
| 8 | roster-cli | 40 | `talaria workflow roster --profile fixture --json` exits 0 and its `version`, `report.providers` and `report.tiers` parse with `jq`; the required `paths.{talaria_home,runs_dir,workflows_dir,drafts_dir}` and `workflows[].path` fields are present and point inside the suite's scratch configuration (P5-09 owns those fields for workflow authors); only the scrubbed projection, with those values substituted, is ever committed as a fixture |
| 9 | list-cli | 30 | `talaria workflow list` on the empty scratch runs folder prints `No workflow runs found.`, exit 0; `--json` prints `[]` (P5-06 REQ-16) |
| 10 | no-gateway | 40 | with `HERMES_HOME` at an empty folder: the suite writes its fixture script `one.js` (the one-agent script of P5-06 UAT-1, copied from the candidate's P5-04 test data at build time and stored under `fixtures/screens/phase5/`) and `workflow run` of it fails with a gateway-not-found line and exit 1 (not the exit-2 refusal of an unparsable script), creating no run folder; `workflow list` still exits 0 (P5-06 UAT-7) |

The sum of limits is 410 s. Steps 3 and 4 depend on texts that two source issues own (section 9.5). A step that would need a model call beyond the two ordinary turns of steps 2 and 6 does not belong in the suite.

### 9.4 Exact reason texts

Every pass predicate has one exact reason, in `steps5.rs`, in the form `FAIL <step> (<reason>)`. Within a step the predicates are checked in table order and the first failure is the reason (no later reason masks it). The complete list: startup: `talaria exited with code <n> before showing its screen`, `the composer was not shown`; plain-answer: `the reply did not carry the token`, `a turn did not settle`; workflows-empty: `the empty-runs text was not shown`, `Esc did not return to the idle composer`; ultracode-status: `the status text was not shown` (the suite's scratch profile holds no skill, so the expected text is exactly P5-09 spec 7.3 N3's not-found form `Ultracode is off for this session. Skill: not found in profile fixture.`), `a turn started`, `the toggle did not stay off`; quit: `Talaria did not exit`, `the token did not survive`; resume: `the token was not shown after resume`, `the fresh token did not answer`, `Talaria did not exit on /quit`, `the shell was not shown`; builtins-cli: `install did not print the expected lines`, `list did not show the four workflows`, `check did not refuse the missing tiers`; roster-cli: `roster did not exit 0 or parse`, `a required path field is missing or outside the scratch configuration`; list-cli: `the empty-list text was not shown`, `the JSON list was not empty`; no-gateway: `run did not fail with the gateway line`, `run created a run folder`, `list did not exit 0`; any step: `deadline reached`, `the step did not start idle`, `the step did not end idle`. A text that differs from its source issue is a FAIL with that reason and the pane text, never relaxed.

### 9.5 Talaria screen texts the steps depend on

All are constants in `tools/smoke/src/wording5.rs`, commented in plain words without planning codes, and each names its source issue: `no runs yet — talaria workflow run starts one` (P5-05 REQ-22); the `/ultracode status` text (P5-09 REQ-22, not quoted in the issue: taken from the owner's specification at build time); the `created` and `unchanged` lines and `installed` marker (P5-08 UAT-1); `missing tiers:` (P5-08 UAT-5); `No workflow runs found.` (P5-06 REQ-16). The source issues' specifications are the wording authority, never the binary: a missing literal is added from the owner's specification by a correction to this unit specification.

### 9.6 Time budget and defaults

Deadline 600 s; step limits as in 9.3; a `--step-scale` option as earlier suites have; a deadline that fires names the running step (`FAIL <step> (deadline reached)`).

### 9.7 The command steps

Steps 7 to 10 run in the first pane's shell after step 6 (`resume`) has quit Talaria and the shell prompt has been read (a full-run test rejects a command step while Talaria still owns the pane). Each sets `TALARIA_CONFIG_DIR` and `TALARIA_RUNS_DIR` to folders inside the suite's scratch folder before the command and prints the exit code on its own line (`echo "exit=$?"`). Step 10 sets `HERMES_HOME` for that one command only.

### 9.8 Screens and tool output to record

Recordings of the real screens and command outputs for each step are fixtures under `tools/smoke/tests/fixtures/screens/phase5/`, recorded from the `main` binary, and are never evidence. Each fixture passes the public-safety scan.

### 9.9 Tests in detail

Files under `tools/smoke/tests/`, written first in this order (the first commit also holds the `Phase5` variant and stubs whose bodies panic `not implemented`). The matrix is finite: for each step, one pass test and one fail test per reason in section 9.4's table (the fail test loops over that step's reasons and asserts the exact `FAIL <step> (<reason>)` text and that the run stops), plus the named full-run, deadline, precondition, cleanup, signal and Talaria-to-shell transition tests in the table below; every listed function must fail on the stubs for its named reason before any implementation.

| # | File and functions | Asserts |
|---|---|---|
| 1 | `screen5.rs`: `overlay_title_found_with_profile` first, then one function per helper | helpers on the recorded screens |
| 2 | `steps5.rs`: a pass and a fail test per step (each fail test loops over every reason of its step), then `a_full_run_returns_to_idle_between_steps`, `a_failing_step_stops_the_run_and_cleans_up`, `deadline_names_the_step`, `a_missing_precondition_is_a_fail_not_a_skip`, `live_require_does_not_change_a_phase5_result` | `run_smoke_5` on `FakeTerminal` and `ManualClock` |
| 3 | `typed_input5.rs`: `nothing_typed_contains_unattended` first, `no_line_runs_hermes_update_or_config_set` last, and one asserting the only slash lines sent are `/workflows`, `/ultracode status` and `/quit` | every key of a full run is on the list |
| 4 | `cleanup5.rs`: `tab_closed_after_pass` first | tab and folders removed on every path |
| 5 | `cli5.rs`: `help_lists_phase5_suite` first; options, preflight, exit codes 0, 1, 2 and never 3 | command line |
| 6 | `fixtures_public_safe5.rs`: `phase5_fixtures_hold_no_paths_or_names` | fixtures safe (passes at once) |
| 7 | `live_smoke5.rs`: `live_phase5_smoke_in_herdr` (`#[ignore]`) | real run: exit 0 and `^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$`; fails, never skips, outside Herdr |

Earlier phases' test files are not edited.

### 9.10 Cleanup, signals, safety

Tab and scratch folders are removed on every path, including a signal; the suite sends no `unattended`, never `hermes update` or `config set`, no confirming key, no `--restart`; Ctrl+C is the only signal it sends, and only to its own pane.

### 9.11 README additions

`tools/smoke/README.md` gets a `phase5` section: what it checks, the ten steps, the preconditions, and that it writes nothing outside its scratch folder.

### 9.12 Journal entries

One entry in `docs/engineering-journal/LEARNINGS.md` if any step needed a rule not written here (for example a read source that lagged).

## 10. Questions the operator owns, and facts not verified

### 10.1 The shape of the record

The record is shape A (the four files of section 4) when `PH4-ACC`'s merged pull request has a `phase-4.md`, otherwise shape B. Shape B is fully defined here so the parent's shape gate needs no further interpretation: no acceptance files are created; the pull request body holds the same 84-row table (same ids, personas and PASS rule) and the evidence sections, the checker and its tests are not built (AC-4 is then checked by the reviewer's row-by-row read, recorded on the parent), AC-5 counts the 84 rows and five header lines in the pull request body, AC-7/AC-10/AC-11/AC-15 use the same greps against that body text, the six UAT scenarios of this issue are unchanged, and any missing table or evidence fails closed. The lead posts the shape on [`PH5`](https://github.com/infiquetra/talaria/issues/218) before the freeze.

### 10.2 Decisions, to confirm on the parent issue before the candidate is frozen

1. Three providers. OPC-1 needs three providers signed in on `fixture` (or a throwaway profile the operator names). If the operator has only two, the choices are: sign in a third (free tiers exist), allow two reviewers to share a provider (this changes the roadmap's check and needs the operator's written agreement), or accept that the row is BLOCKED.
2. The 600 s smoke deadline and the XF-10 limits (carried from phase 4) are proposals.
3. Whether the provider's own usage page is wanted as a second source for OPC-5 (the recording is the gate; the page is optional).
4. Whether the operator's own repository branch, or the sample repository, is used in OPC-2 (default: the sample).

### 10.3 Facts not verified while drafting (check at build time, record the result)

1. The field names of the frame-log format used by `outcount` (`docs/formats/frame-log.md`, version 3): verified on candidate `da6907c` — outbound request frames carry `dir == "out"`, `frame.type == "req"`, `frame.method`, and `frame.params` (`idempotencyKey` or `clientRequestId`).
2. The text of `/ultracode status` (P5-09 spec 7.3 N3, quoted in 9.4) and the progress view's stop-run key `Shift+X` (P5-05 REQ-19): verified on candidate `da6907c` in `AUTO-14`, `AUTO-15`, `XF-6`, and `XF-8`.
3. Whether `interrupted` appears in `wf list` within 10 seconds of the kill in `OPC-3` (P5-06 REQ-6a): verified in `OPC-3` — killing the detached runner released the kernel `flock` immediately and `workflow list --json` reported `"status":"interrupted"` within 10 seconds.
4. Whether Hermes reports a cost for the `ollama-cloud` provider (so whether `OPC-5` sees `cost unknown`): verified on Hermes `v0.22.4` in `OPC-1` and `OPC-5` — `ollama-cloud`, `deepseek`, and `gemini` report `cost_status: "unknown"`, so `money_spent_usd` is `null` and `/workflows` renders `cost unknown` (never `$0.00`).
5. Whether the `skills` folder of the profile is the one `P5-09` writes (`skills/talaria-workflows`), and whether it moves any `snap` digest: verified — installing `talaria-workflows` adds `skills/talaria-workflows` (`snap_neg_cmp=1`), and removing `skills/talaria-workflows` plus staging `.locks` restores `snap_cmp=0` and `cron_cmp=0`.
6. The earlier suites' `PASS` line counts and limits (copied from `PH4-ACC` at its freeze): verified in `AUTO-8` through `AUTO-13` (11, 8, 15, 12, 17, and 14 `PASS` lines respectively), with `phase5` adding 10 `PASS` lines in `AUTO-14` and `AUTO-15`.
7. Whether `limits.max_in_flight` is the staffing key for the width (P5-04 section 3) and the default width is 10: verified in `XF-9` (`max_in_flight=3` caps concurrency at 3; omitting `limits.max_in_flight` uses the default width of 10).

## Appendix A: the scenarios of the accepted issues, indexed

| Row | Title | Persona | Runner in the source | Runs as |
|---|---|---|---|---|
| P5-01 UAT-1 | A review-shaped workflow runs end to end against the mock | workflow-runner | agent tester in Herdr | agent |
| P5-01 UAT-2 | A script that reads the clock fails with the right line and advice | scripter | agent tester in Herdr | agent |
| P5-01 UAT-3 | A non-literal header, or a script using import(), is refused before anything runs | scripter | agent tester in Herdr | agent |
| P5-01 UAT-4 | A Claude Code-format script transfers; a Claude Code-only option is refused clearly | workflow-runner | agent tester in Herdr | agent |
| P5-01 UAT-5 | A runaway loop is stopped and the process stays healthy | scripter | agent tester in Herdr | agent |
| P5-01 UAT-6 | Ctrl+C cancels a run with agents in flight | workflow-runner | agent tester in Herdr | agent |
| P5-02 UAT-1 | A killed run resumes without re-running finished agents | workflow-runner | agent tester in Herdr | agent |
| P5-02 UAT-2 | A crash at any step, operator answers included, never sends work twice | workflow-runner | agent tester in Herdr | agent |
| P5-02 UAT-3 | An agent lost in a gateway restart is shown, not re-run (negative) | fleet-lead | agent tester in Herdr | agent |
| P5-02 UAT-4 | A second runner on the same run is refused (negative) | workflow-runner | agent tester in Herdr | agent |
| P5-02 UAT-5 | Hermes deduplicates and keeps a workflow agent's turn alive (live) | developer | either (agent tester in Herdr when the gateway is already running; the operator sets up the profile) | agent |
| P5-02 UAT-6 | A damaged journal is refused with its line number (negative) | workflow-runner | agent tester in Herdr | agent |
| P5-03 UAT-1 | A text agent runs as a hidden session on the requested model | workflow-runner | agent tester in Herdr | agent |
| P5-03 UAT-2 | A schema agent gets one correction and returns an object | workflow-runner | agent tester in Herdr | agent |
| P5-03 UAT-3 | A dangerous command is denied without anyone waiting | workflow-runner | agent tester in Herdr (needs the operator for the approvals precondition) | agent |
| P5-03 UAT-4 | Killing the runner mid-call loses no work and runs nothing twice | fleet-lead | agent tester in Herdr | agent |
| P5-03 UAT-5 | An unknown toolset is refused by Hermes and no prompt is sent | workflow-runner | agent tester in Herdr | agent |
| P5-03 UAT-6 | No gateway gives a plain error | workflow-runner | agent tester in Herdr | agent |
| P5-04 UAT-1 | The roster shows what a workflow can use | configurer | agent tester in Herdr | agent |
| P5-04 UAT-2 | A tier on a model nobody offers stops the run before it starts | workflow-runner | agent tester in Herdr | agent |
| P5-04 UAT-3 | A twelve-agent fan-out never runs more than the width allows | workflow-runner | agent tester in Herdr | agent |
| P5-04 UAT-4 | The token budget stops new agents and says why | workflow-runner | agent tester in Herdr | agent |
| P5-04 UAT-5 | A rate-limited provider holds its agent until the reset, then admits it | fleet-lead | operator | operator |
| P5-04 UAT-6 | Editing the staffing table between run and resume is reported | configurer | agent tester in Herdr | agent |
| P5-05 UAT-1 | A finished run reads correctly from its journal alone | workflow-runner | agent tester in Herdr | agent |
| P5-05 UAT-2 | Steer and stop a running agent from the tree | workflow-runner | agent tester in Herdr | agent |
| P5-05 UAT-3 | Pause and stop requests move the run line honestly | fleet-lead | agent tester in Herdr | agent |
| P5-05 UAT-4 | Open an agent without disturbing the run | developer | agent tester in Herdr | agent |
| P5-05 UAT-5 | A corrupt journal and a missing gateway degrade, never mislead (negative) | workflow-runner | agent tester in Herdr | agent |
| P5-06 UAT-1 | Run a workflow in the foreground and see it finish | workflow-runner | agent tester in Herdr | agent |
| P5-06 UAT-2 | Detach, close the terminal, and find the run finished | fleet-lead | agent tester in Herdr | agent |
| P5-06 UAT-3 | Kill the runner mid-run and resume without repeating finished agents | fleet-lead | agent tester in Herdr | agent |
| P5-06 UAT-4 | Stop a running workflow and confirm no agent still works | workflow-runner | agent tester in Herdr | agent |
| P5-06 UAT-5 | Answer a parked call with the resolve verb (negative recovery) | workflow-runner | agent tester in Herdr (needs the operator for the gateway restart that makes an admission unknown) | paired |
| P5-06 UAT-6 | A bad tier refuses the start and creates no run (negative case) | workflow-runner | agent tester in Herdr | agent |
| P5-06 UAT-7 | List and replay with no gateway (negative case) | workflow-runner | agent tester in Herdr | agent |
| P5-07 UAT-1 | Two parallel agents edit without colliding | workflow-runner | agent tester in Herdr | agent |
| P5-07 UAT-2 | A resumed run reuses the same worktree from a different folder | workflow-runner | agent tester in Herdr | agent |
| P5-07 UAT-3 | A non-git folder fails plainly and runs nothing | workflow-runner | agent tester in Herdr | agent |
| P5-07 UAT-4 | Removing a dirty worktree is refused, then forced | workflow-runner | agent tester in Herdr | agent |
| P5-08 UAT-1 | Install and list the built-ins on a fresh configuration | developer | agent tester in Herdr | agent |
| P5-08 UAT-2 | Review a branch live on the free model | workflow-runner | agent tester in Herdr | agent |
| P5-08 UAT-3 | Fix failing tests without touching the checkout | developer | agent tester in Herdr | agent |
| P5-08 UAT-4 | Refuse a dangerous-looking ref before any agent starts | workflow-runner | agent tester in Herdr | agent |
| P5-08 UAT-5 | Check staffing before a three-provider run | configurer | either | agent |
| P5-08 UAT-6 | Ultracode end to end on the mock backend | builder | agent tester in Herdr | agent |
| P5-09 UAT-1 | Install, re-install, refuse an edit, force with backup | configurer | agent tester in Herdr | agent |
| P5-09 UAT-2 | Roster envelope for an agent, and a clear failure without a gateway | scripter | agent tester in Herdr | agent |
| P5-09 UAT-3 | Toggle ultracode on and see a routed prompt | developer | agent tester in Herdr | agent |
| P5-09 UAT-4 | Refusal when the skill is missing, and nothing is lost | developer | agent tester in Herdr | agent |
| P5-09 UAT-5 | A substantive task runs as a workflow across agents | workflow-runner | operator (script quality depends on the lead model, and the run spends tokens) | operator |
| P5-09 UAT-6 | The keyword starts one workflow without turning the toggle on | developer | agent tester in Herdr | agent |
