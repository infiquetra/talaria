# PH1-ACC: Phase 1 acceptance: the smoke suite on the candidate, then a day of real coding by the operator

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order, and how the result is recorded. This document holds what the people running the acceptance would otherwise have to guess: how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (written and run while drafting this document, on macOS; re-run them on the machine that builds the unit), the exact steps and pass conditions of the eight cross-feature scenarios and the six operator scenarios, the run order, the rules for failures and a new candidate, the smoke suite coverage map and the design for extending it later. Appendix A holds the verbatim text of the 49 scenarios of the accepted issues, copied from their final bodies; the source issue's own text wins if it was edited after this document was written.

Sections are numbered so the issue can point at them. Handoff documents are linked relatively: [the roadmap's phase 1 operator check](../07-roadmap-and-work-units.md#phase-1-operator-check), [the P1-13 roadmap entry](../07-roadmap-and-work-units.md#p1-13--live-smoke-test-in-herdr), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here), [driving Talaria through Herdr by hand](../research/u13-hands-on-talaria-drive.md).

## 1. What this unit is for, in plain terms

Eight children of the phase 1b capability each passed their own scenarios on their own commit. A passing child proves its parts, not the whole: the renderer, the composer, the approval cards, the status line, quit and resume and the live Hermes gateway have never run together except in the smoke suite of unit P1-13, which checks five behaviours in under five minutes. This unit runs everything again on one commit, adds the checks that only make sense when the parts are combined, and asks the operator to use the result for a working day.

The operator's check in the roadmap has five clauses: approvals and clarifications work; streaming feels instant while typing; the terminal's own search and copy work on the transcript; quitting and resuming loses nothing; nothing on screen is stale or garbled. Each clause is a row here (OP-1 to OP-5) with counts and observable pass conditions, so "it felt fine" is never the evidence. OP-6 is the day log and the verdict.

The Python client spent 44 percent of the lines it added on acceptance records ([02-current-state.md](../02-current-state.md#why-it-ended-up-here), item 4). This unit therefore writes two markdown files, one short checker script (about 55 lines) and its test, and nothing else. There are no receipts, schemas, signatures or evidence bundles.

Draft ADR-0012 ([verification is proportionate](../adr-drafts/0012-verification-is-proportionate.md), decision 4) says no per-release receipt schemas, digest-bound evidence trees or release-record tests, and the repository reset ([08-risks-and-open-questions.md](../08-risks-and-open-questions.md), decision 2) removes the Python era's `docs/acceptance/` from `main`. This unit recreates a small `docs/acceptance/v0.7.0/` on purpose, so the tension is stated and not hidden. It is a phase gate record, not a release receipt: two markdown files, no schema, no digests, and a checker of about 55 lines whose tests exist only to keep that checker honest. The shape is the drafter's proposal and the operator's to confirm on the parent issue before the candidate is frozen. If the operator declines it, the fallback is to drop the checker, its tests and the folder, and to put the 70-row table in the pull request body, with the checker's rules applied by the reviewer; the issue is then edited before anything is built.

## 2. Facts

### 2.1 The `herdr` command line

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) were checked by running Herdr 0.9.4 on macOS on 2026-10-22. The commands used here (`herdr tab create|list|close`, `herdr pane run|send-text|send-keys|read|wait-output|process-info|split`) are the ones listed there. Re-check them with the Herdr in use and write its `herdr --version` in the record header. Three points matter most: `pane run` types text and Enter together; `pane read` has sources `visible` (the screen), `recent` (scrollback with soft wraps) and `recent-unwrapped` (scrollback with wraps joined); and a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. Whether `visible` or `recent-unwrapped` is stale for a full screen program is a **verify** item: run `echo READ-CHECK` in the pane first and use the source that shows it; a read that lacks text just typed is stale and is never evidence.

### 2.2 Hermes facts used (read at Hermes v0.22.4, commit 4d04303; re-check at the release in `crates/hermes-contract/PINNED` before relying on them)

- `approvals.mode` is `manual`, `smart` or `off`; the default is `smart` and the default `timeout` is 300 s (`hermes_cli/config_defaults.py` lines 1694 and 1695 at Hermes v0.22.4). `smart` lets a guardian model approve a flagged command with no card. The valid modes are listed in `tools/approval_context.py` line 263 and a hosted-room policy overrides the configured mode (lines 295 to 299 of that file).
- `rm -rf .pytest_cache` matches the recursive-delete pattern `\brm\s+-[^\s]*r` (`tools/approval_detection.py` line 230), so with `manual` it produces an approval card.
- An unanswered approval is denied after `approvals.timeout` (default 300 s).
- `prompt.submit` accepts `finite` and `unattended` (`gateway/session_controls.py` line 751). In `gateway/session_finite.py` (lines 34 to 57), `unattended: true` is valid only together with `finite: true`, and over the wire only for the local command-line principal on its own session; where it applies, the turn is auto-approved (`tools/approval.py` lines 325 to 333, `_yolo_active`), except for Hermes's hardline floors, which run first. No scenario here sends `unattended`.
- `/quit` in Talaria detaches; it sends no `end_reason`, so the session keeps running in the gateway (P1-12 issue, settled decision 1).

### 2.3 What the source issues provide

The demo programs and ignored live tests are named in Appendix A. The ones this document relies on: `inline_demo` and the `tmux_scrollback` test (P1-06); `markdown_demo` (P1-07); `composer_demo` and the `composer_pty` tests (P1-08); `prompt_cards` (P1-09); `status_demo` (P1-10); `talaria-bench`, `fake-tui` and `talaria --bench-replay` (P1-11); the `talaria` binary, its options and the `live_session` tests (P1-12); `smoke run` (P1-13). Screen words used by the cross-feature scenarios and where they come from: the composer and status line (`minimax-m3.1-flash` shown on the bottom line, P1-12 UAT-1); the approval card (`$ <command>` then numbered choices `once`, `session`, `always`, `deny`, the first highlighted, a digit sends that choice, P1-09); the rule that an approval card takes keys only while the draft is empty and otherwise shows a hint asking the person to clear the draft, with keys going to the composer (P1-09 issue, REQ-15; the exact hint wording is left to the builder, so the scenarios accept any hint containing the word `draft`, and a card that takes a digit while the composer holds a draft is a FAIL, not BLOCKED); `── Resumed: <title> ──` and `Resume this session with:` (P1-12); `stopping…` on a card after Ctrl+C (P1-09).

## 3. Freezing and building the candidate

The lead does this once, after the issues `P1-06` to `P1-13` and the phase 1a capability are closed. Never switch the primary checkout (it may hold untracked material); work in a worktree outside the repository. First the lead reads `crates/hermes-contract/PINNED` at the commit to be frozen and re-reads the Hermes facts of section 2.2 at that release; a fact that changed is corrected here and reported on the issue before the freeze, and the record's `Hermes:` line names that release.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p1-acc                                     # a new folder outside the repository (no TMPDIR needed)
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p1-acc-candidate"   # for the bench runs of AUTO-1 and P1-11 UAT-5; deleted at closeout
cargo build --release --workspace --examples
cargo build --release -p talaria-cli -p talaria-bench
cargo build -p smoke                                       # the debug smoke program that P1-13 UAT-4 runs by path
shasum -a 256 target/release/talaria                       # the binary every row and the operator's day use
git switch -c p1-acc-record                                # the record branch starts at the candidate commit
```

The record branch holds documentation commits only; they change nothing that is built or run (AC-4 proves it). After a new candidate (a fix), the branch is rebased onto the new candidate commit. `main` may move during the run: the record is compared with the candidate, never with `main`.

Shell variables do not carry from one Herdr pane to another. At freeze the lead writes the common settings to a file next to the worktree, and every pane starts in the worktree root (`--cwd`) and runs `. ../acc.env` first. The file holds `CANDIDATE`, `T` (the absolute path of the release binary), `F` (the launch options) and the `PATH` line below. The file's location is never written in the issue, the record or the evidence.

```bash
cat > ../acc.env <<EOF2
CANDIDATE=$CANDIDATE
T=$PWD/target/release/talaria
F='--no-ensure --profile fixture --provider ollama-cloud --model minimax-m3.1-flash'
PATH=$PWD/target/release:\$PATH
EOF2
. ../acc.env; command -v talaria                            # must print this worktree's target/release/talaria
```

Per run settings (each scenario makes its own): `X=$(mktemp -d)` for evidence scratch; the pane helper

```bash
pane_new() { herdr pane split "$HERDR_PANE_ID" --direction right --no-focus --cwd "$1" | jq -r '.result.pane.pane_id'; }
```

`--no-ensure` is on every launch that starts a live session, so that Talaria can never start a gateway. Preflight before stage 1 (once, with the Herdr in use): `herdr --version`; run both pane-split forms, the source issues' (`herdr pane split --current --direction right --cwd .`) and the helper's above, and close the panes they make. A form Herdr rejects is reported on the issue and the rows that need it are `BLOCKED (environment)`. For source rows use the form the source scenario writes; the helper is for the XF rows.

If a code path changes after this point (a fix), the candidate is dead: go back to the start of this section with the new commit.

## 4. The record

File `docs/acceptance/v0.7.0/phase-1.md`. The second commit (after the stub commit) holds this skeleton with every header line filled in with its real value (the angle-bracket text above is what to replace, and the checker refuses a header that still holds it), and `phase-1-evidence.md` holding only its title line (so the final checker's first run on these two files is the red run, test 23: 70 problems). Row order is the order below; do not add other tables.

```markdown
# Phase 1 acceptance record

Candidate: <40 hex characters of the frozen candidate commit>
Hermes: <release tag from crates/hermes-contract/PINNED>
Herdr: <output of herdr --version>
OS: <macos or linux>
Date: <YYYY-MM-DD>

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| AUTO-1 | builder | NOT RUN |  |  |
| AUTO-2 | builder | NOT RUN |  |  |
| AUTO-3 | builder | NOT RUN |  |  |
| AUTO-4 | builder | NOT RUN |  |  |
| AUTO-5 | builder | NOT RUN |  |  |
| AUTO-6 | builder | NOT RUN |  |  |
| AUTO-7 | builder | NOT RUN |  |  |
| P1-03 UAT-1 | developer | NOT RUN |  |  |
| P1-05 UAT-3 | developer | NOT RUN |  |  |
| P1-06 UAT-1 | developer | NOT RUN |  |  |
| P1-06 UAT-2 | developer | NOT RUN |  |  |
| P1-06 UAT-3 | developer | NOT RUN |  |  |
| P1-06 UAT-4 | developer | NOT RUN |  |  |
| P1-06 UAT-5 | scripter | NOT RUN |  |  |
| P1-06 UAT-6 | developer | NOT RUN |  |  |
| P1-07 UAT-1 | developer | NOT RUN |  |  |
| P1-07 UAT-2 | developer | NOT RUN |  |  |
| P1-07 UAT-3 | developer | NOT RUN |  |  |
| P1-07 UAT-4 | developer | NOT RUN |  |  |
| P1-07 UAT-5 | builder | NOT RUN |  |  |
| P1-07 UAT-6 | developer | NOT RUN |  |  |
| P1-08 UAT-1 | developer | NOT RUN |  |  |
| P1-08 UAT-2 | developer | NOT RUN |  |  |
| P1-08 UAT-3 | developer | NOT RUN |  |  |
| P1-08 UAT-4 | developer | NOT RUN |  |  |
| P1-08 UAT-5 | developer | NOT RUN |  |  |
| P1-09 UAT-1 | developer | NOT RUN |  |  |
| P1-09 UAT-2 | developer | NOT RUN |  |  |
| P1-09 UAT-3 | developer | NOT RUN |  |  |
| P1-09 UAT-4 | developer | NOT RUN |  |  |
| P1-09 UAT-5 | developer | NOT RUN |  |  |
| P1-09 UAT-6 | developer | NOT RUN |  |  |
| P1-09 UAT-7 | developer | NOT RUN |  |  |
| P1-10 UAT-1 | developer | NOT RUN |  |  |
| P1-10 UAT-2 | configurer | NOT RUN |  |  |
| P1-10 UAT-3 | configurer | NOT RUN |  |  |
| P1-10 UAT-4 | developer | NOT RUN |  |  |
| P1-10 UAT-5 | developer | NOT RUN |  |  |
| P1-11 UAT-1 | builder | NOT RUN |  |  |
| P1-11 UAT-2 | builder | NOT RUN |  |  |
| P1-11 UAT-3 | developer | NOT RUN |  |  |
| P1-11 UAT-4 | developer | NOT RUN |  |  |
| P1-11 UAT-5 | builder | NOT RUN |  |  |
| P1-12 UAT-1 | developer | NOT RUN |  |  |
| P1-12 UAT-2 | developer | NOT RUN |  |  |
| P1-12 UAT-3 | developer | NOT RUN |  |  |
| P1-12 UAT-4 | developer | NOT RUN |  |  |
| P1-12 UAT-5 | developer | NOT RUN |  |  |
| P1-12 UAT-6 | developer | NOT RUN |  |  |
| P1-12 UAT-7 | developer | NOT RUN |  |  |
| P1-12 UAT-8 | developer | NOT RUN |  |  |
| P1-13 UAT-1 | builder | NOT RUN |  |  |
| P1-13 UAT-2 | builder | NOT RUN |  |  |
| P1-13 UAT-3 | builder | NOT RUN |  |  |
| P1-13 UAT-4 | builder | NOT RUN |  |  |
| P1-13 UAT-5 | builder | NOT RUN |  |  |
| XF-1 | developer | NOT RUN |  |  |
| XF-2 | developer | NOT RUN |  |  |
| XF-3 | developer | NOT RUN |  |  |
| XF-4 | developer | NOT RUN |  |  |
| XF-5 | developer | NOT RUN |  |  |
| XF-6 | developer | NOT RUN |  |  |
| XF-7 | developer | NOT RUN |  |  |
| XF-8 | developer | NOT RUN |  |  |
| OP-1 | developer | NOT RUN |  |  |
| OP-2 | developer | NOT RUN |  |  |
| OP-3 | developer | NOT RUN |  |  |
| OP-4 | developer | NOT RUN |  |  |
| OP-5 | developer | NOT RUN |  |  |
| OP-6 | developer | NOT RUN |  |  |
```

Rules: `result` is exactly `PASS`, `FAIL`, `BLOCKED (<cause>)` (causes: `environment`, `not built`, `no access`) or `NOT RUN`. A PASS row carries the first 7 or more characters of the candidate in `commit` and the link `phase-1-evidence.md#<id lower case, spaces to hyphens>` in `evidence`; for example the row `P1-06 UAT-1` links `phase-1-evidence.md#p1-06-uat-1`. A row that is not PASS may leave `evidence` and `commit` empty but its evidence section should say why. BLOCKED and NOT RUN never count as a pass; there is no waiver state.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-1-evidence.md` starts with `# Phase 1 acceptance evidence` and then holds exactly one section per row, in record order:

````markdown
## P1-06 UAT-1

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-11-02.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

The checker requires, for every PASS row: the `Candidate:` line (7 or more hex characters equal to the start of the candidate, then a full stop or space), at least one fenced block with a non-blank line, and no block of more than 60 lines. A section may hold several blocks, each of 60 lines or fewer (only the length of one block is limited, never the number of blocks), and a line starting with `## ` inside a block does not end the section. Two sections for one id are a problem.

**When the capture is longer than 60 lines** (a 400-line scrollback read, a 200-line smoke output, two pane reads) the section keeps, in blocks of at most 60 lines: (1) the commands that check the read and their outputs (a `grep -c`, `count_check`, the exactly-once counts, an `awk` line); (2) the first 15 and last 15 lines of the read; (3) every line the source row's Then clause names (for example `line 0001` and `line 0300`, the `Resumed` line, the `OK` line). Each cut is marked with a line `[... <n> lines omitted; checked by the commands above ...]`. The full read stays in the scratch folder and is never committed. An excerpt may not hide a failing line: the checking command ran over the whole read, and the section says which file held it. Two pane reads are two blocks, each cut the same way. The reviewer checks what the script cannot: the runner and date line, the run list, the three-run limit, the operator's comment URL, the row order and persona column, that every row that is not PASS says why, and that no evidence section exists for an id that is not a row. An operator row holds the operator's words (what they did, counts, the terminals), the URL of the operator's comment on the issue, and for OP-6 the day log of section 7.4. A row run more than once has one section holding the blocks of every run in order, each earlier failure named with its cause (for example a provider rate-limit error, never "flaky"); each run repeats the line `Run <k> of <n>` before its blocks. At most three runs per row on one candidate (ruling R-3 of section 10.1 says what counts as a run and who decides). The `Runner:` word is a role, `agent tester` or `operator`, never a person's name or login. AUTO-1 and the day log carry the `shasum -a 256` of the release binary.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output (the result lines). Source rows: what the source scenario's Evidence line asks for. XF and OP rows: what spec 7.3 and 7.4 name. Screenshots are attached to the operator's comment, not committed.

### 5.3 Scrubbing (before anything is pasted)

```bash
scrub() {
  sed -e "s#$HOME#<home>#g" -e "s#$(id -un)#<user>#g" \
      -e "s#${TMPDIR%/}#<tmp>#g" -e "s#$(cd "${TMPDIR:-/tmp}" && pwd -P)#<tmp>#g" \
      -e "s#$PWD#<worktree>#g"
}
herdr pane read "$PANE" --source recent-unwrapped --lines 200 | scrub > "$X/evidence.txt"
```

Then read the result: no home directory, user name (a login name under four characters cannot be scrubbed safely: say so in the section), temporary path, host name, provider key or ticket, and no profile name other than `default` or `fixture`. If a line shows any other profile name, replace it with `<profile>`. A captured line that starts with three backticks gets one leading space (the checker takes such a line for a fence). The same scrub applies to an operator's comment before it is copied (ruling R-2): the agent replaces only identifiers (home directory, login name, temporary path, host name, unknown profile name) with the same placeholders, never a count, a result or a word of judgement, and ends the section with a line `Scrubbed: <the kinds replaced>` or `Scrubbed: nothing`; the comment URL stays so that anyone can compare. A comment that holds a ticket, key or other secret is not copied: the agent asks the operator to edit it first. The checker catches paths only; AC-11 also greps the `--profile` values (against an allow-list of names) and the login name of everyone who ran a row. A clean grep proves only that those shapes are absent: the reading step is the control, and the same identifier class has leaked from evidence trees before.

## 6. The checker and its tests

### 6.1 Behaviour

`check-record.sh [record [evidence]]` (defaults: `phase-1.md` and `phase-1-evidence.md` next to the script). It holds the list of 70 row ids in one `IDS=` line. It prints one line per problem and then `ACCEPTANCE_RESULT: PASSED (<n> of 70 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 70 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. Problems: a row that is not PASS (the line is `<id>: <result>`); a result that is not one of the four forms; a PASS row whose commit is not a prefix of the candidate or is shorter than 7 characters; a PASS row whose evidence link is not exactly `phase-1-evidence.md#<slug>`; a PASS row whose evidence file has no `## <id>` section with a fenced block holding at least one non-blank line ("empty" means no text inside the fence); a fenced block of more than 60 lines; an evidence section that does not name the candidate (`Candidate: <7 hex>.`); a missing, extra or duplicated row; two evidence sections for one id; a missing or malformed `Candidate:` line; a header line `Hermes:`, `Herdr:`, `OS:` (must be `macos` or `linux`) or `Date:` (`YYYY-MM-DD`) that is missing or still holds a `<`...`>` placeholder; and, counted as one problem, a local path (the folder names `Users`, `home`, `private`, `var` or `tmp` between slashes, followed by a letter) anywhere in either file. It never touches the network or git. What it cannot check is listed in section 5.1 (reviewer checks). The skeleton commit carries real header lines (the lead knows the release, the Herdr version, the platform and the date at freeze), so its red run is exactly 70 problems, one per NOT RUN row.

If a command-saving proxy wraps shell commands on the machine, run the checker and its tests through the proxy's pass-through mode or in a plain pane and paste the raw output.

### 6.2 Reference implementation

Written for POSIX `sh` and `awk`. Exercised while drafting on macOS (awk version 20200816): the 22 cases below pass, and the stub below fails every one of them on its stated expectation. The script uses no brace counts in regular expressions (some `awk` builds lack them) and writes the backtick fence as `\140\140\140`. It has not been run with GNU `awk` or `mawk`: run the 21 cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 1 acceptance record. Usage: check-record.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-1.md}
EVI=${2:-$DIR/phase-1-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-record: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|P1-03 UAT-1|P1-05 UAT-3|P1-06 UAT-1|P1-06 UAT-2|P1-06 UAT-3|P1-06 UAT-4|P1-06 UAT-5|P1-06 UAT-6|P1-07 UAT-1|P1-07 UAT-2|P1-07 UAT-3|P1-07 UAT-4|P1-07 UAT-5|P1-07 UAT-6|P1-08 UAT-1|P1-08 UAT-2|P1-08 UAT-3|P1-08 UAT-4|P1-08 UAT-5|P1-09 UAT-1|P1-09 UAT-2|P1-09 UAT-3|P1-09 UAT-4|P1-09 UAT-5|P1-09 UAT-6|P1-09 UAT-7|P1-10 UAT-1|P1-10 UAT-2|P1-10 UAT-3|P1-10 UAT-4|P1-10 UAT-5|P1-11 UAT-1|P1-11 UAT-2|P1-11 UAT-3|P1-11 UAT-4|P1-11 UAT-5|P1-12 UAT-1|P1-12 UAT-2|P1-12 UAT-3|P1-12 UAT-4|P1-12 UAT-5|P1-12 UAT-6|P1-12 UAT-7|P1-12 UAT-8|P1-13 UAT-1|P1-13 UAT-2|P1-13 UAT-3|P1-13 UAT-4|P1-13 UAT-5|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|OP-1|OP-2|OP-3|OP-4|OP-5|OP-6'
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
  if (evid != "phase-1-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-1-evidence.md#" slug(id))
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

The test script builds a good record and 21 variations in a temporary folder, runs the checker on each and compares the exit code and one expected text. It prints `CASE <name>: ok` or `CASE <name>: FAILED (...)` and a final `CHECK_RECORD_TESTS: PASSED` or `CHECK_RECORD_TESTS: FAILED (<n>)`. It reads the id list out of the checker (`IDS='...'` line), so the two cannot drift; the `all-pass` case expects the text `70 of 70`, so a dropped id fails it; the evidence block of `XF-6` starts with a line `## not a heading`, so a checker that ends a section at every `## ` line fails this case too. The 22nd case, `duplicate-section`, appends a second section for `AUTO-2` and expects `AUTO-2: the evidence file has 2 sections for it`. Cases and expectations are the table in the issue. The stub the test author commits first is

```sh
#!/bin/sh
# stub: the real checker replaces this file
IDS='<the same IDS line as in the reference checker>'
echo "ACCEPTANCE_RESULT: FAILED (stub)"; exit 1
```

so every case fails on its expectation (a stub without the `IDS=` line would make every case build an empty record and fail for the wrong reason). Copying the reference test script below is allowed and expected; independence means the author is not the developer who writes the checker, and the lead names the author at launch. Reference implementation:

````sh
#!/bin/sh
# Tests for check-record.sh. Run: sh docs/acceptance/v0.7.0/check-record.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-record.sh"
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
      echo "| $id | developer | $res | phase-1-evidence.md#$slug | $com |"
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
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (70 of 70 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "OP-3" "NOT RUN";               expect not-run-fails 1 "OP-3: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-4" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-4: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P1-09 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-6' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-6: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P1-12 UAT-6";               expect no-evidence-section 1 "P1-12 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-1-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
{ cat "$T/ev.md"; printf 'see /%s/someone/project\n' Users; } > "$T/ev3.md"; expect local-path 1 "a local path appears" "$T/good.md" "$T/ev3.md"
expect missing-file 2 "not found" "$T/nope.md" "$T/ev.md"
make_evidence "$T/ev4.md" "AUTO-2"; section AUTO-2 "" >> "$T/ev4.md"; expect empty-evidence-block 1 "AUTO-2: the evidence file has no section" "$T/good.md" "$T/ev4.md"
make_evidence "$T/ev5.md" "AUTO-2"; section AUTO-2 "$(seq 1 61)" >> "$T/ev5.md"; expect long-evidence-block 1 "AUTO-2: a fenced block of the evidence is longer than 60 lines" "$T/good.md" "$T/ev5.md"
make_record "$T/k.md" "AUTO-3" PASS 012345;            expect short-commit 1 "AUTO-3: commit 012345 is not the candidate" "$T/k.md" "$T/ev.md"
{ cat "$T/good.md"; printf 'see /%s/someone/project\n' tmp; } > "$T/l.md"; expect local-path-in-record 1 "a local path appears" "$T/l.md" "$T/ev.md"
sed 's/^Hermes: .*/Hermes: <pinned release tag>/' "$T/good.md" > "$T/m.md"; expect placeholder-header 1 "Hermes: the header line is still a placeholder" "$T/m.md" "$T/ev.md"
sed 's/^Candidate: 0123456\./Candidate: deadbee./' "$T/ev.md" > "$T/ev6.md"; expect evidence-names-another-candidate 1 "AUTO-1: the evidence section must name the candidate" "$T/good.md" "$T/ev6.md"
make_evidence "$T/ev7.md" none; section AUTO-2 ok >> "$T/ev7.md"; expect duplicate-section 1 "AUTO-2: the evidence file has 2 sections for it" "$T/good.md" "$T/ev7.md"
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-1.md" && cp "$T/ev.md" "$T/d/phase-1-evidence.md"
out=$(sh "$T/d/check-record.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record (test 23): the skeleton of section 4, with the real candidate and real header lines, is committed second (after the stub commit, before the checker), and once the real checker exists `sh check-record.sh` is run on the skeleton's two files (taken from that commit with `git show`, as UAT-2 of the issue does) and must end `ACCEPTANCE_RESULT: FAILED (70 problems, 0 of 70 rows PASS)` and exit 1 (checked while drafting). The point of the order is that the empty record exists before any result; the stub cannot give the 70 problems, so this red run needs the final checker and is pasted from UAT-2. The pull request pastes both red runs: the 22 cases against the stub, and this one.

## 7. The scenario catalogue

Every row has an id, a persona, a runner, steps, an observable result and the evidence to capture. Runner words: `agent tester` is an agent working in a Herdr pane; `operator` is the human. All commands run at the worktree root unless a row says otherwise.

### 7.1 AUTO rows (persona builder; agent tester)

Each row lists its commands, what makes it PASS and what to paste as evidence. `R=infiquetra/talaria`.

**AUTO-1: project check and the candidate's continuous integration (CI) results.** The `bench` workflow triggers on `pull_request` and `workflow_dispatch` (P1-11 REQ-20), and a pull request run reports on the pull request's own commit, not on a merge commit of `main`; so the check named `bench` exists on the candidate only after a dispatch run on the candidate branch (the Linux run; the macOS run is the operator's row P1-11 UAT-5).

```bash
gh workflow run bench.yml --ref p1-acc-candidate -R "$R"
# wait until the newest run on that branch has headSha equal to $CANDIDATE, then watch it:
gh run list -R "$R" --workflow bench.yml --branch p1-acc-candidate --limit 1 --json databaseId,headSha
gh run watch <databaseId> -R "$R" --exit-status
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
gh api --paginate "repos/$R/commits/$CANDIDATE/check-runs?filter=latest" --jq '.check_runs[] | "\(.name) \(.conclusion)"'
shasum -a 256 target/release/talaria
```

PASS: the dispatch run's `headSha` equals the candidate and `gh run watch` exits 0; the cargo set exits 0; every CI line ends `success` or `skipped`, and a line `bench success` is among them. Evidence: the run id and `headSha`, the exit codes, the CI lines, the checksum.

Details (ruling R-10). The job is named `bench` in `.github/workflows/bench.yml` (P1-11 specification section 8), so its check run is `bench`. The workflow's `runner` input has the options `ubuntu-latest` and `macos-14` and the default `ubuntu-latest`, so the dispatch above is the Linux run; the macOS run (P1-11 UAT-5) passes `-f runner=macos-14`. `filter=latest` lists the newest check run of each name on the commit, so a run superseded by a rerun is not listed. Any listed line that ends otherwise (`failure`, `cancelled`, `neutral`, `timed_out`, `action_required`, or no conclusion after 30 minutes) stops PASS, whichever app or workflow it belongs to: from a Talaria workflow it is a FAIL; from anything else it is `BLOCKED (environment)` until the lead has had it rerun or removed and said so on the issue. If P1-11's job is no longer named `bench` at the candidate, the row is `BLOCKED (not built)` and the lead reports it.

**AUTO-2: replay suites** (tests that play recorded Hermes frames back).

```bash
cargo test -p hermes-client --test fixtures_replay --test replay_source --test reconnect
cargo test -p talaria-core --test session_replay --test session_snapshot
cargo test -p talaria-tui --test markdown_replay
cargo test -p talaria-app --test prompts_replay
cargo test -p talaria-cli --test bench_replay
```

PASS: each exits 0 and each target prints a line matching `^test result: ok\. [1-9][0-9]* passed` (a target that ran no test prints `0 passed` and is a FAIL). If a named target does not exist at the candidate the row is `BLOCKED (not built)`. Evidence: the `test result` lines.

**AUTO-3: performance budgets on the real release binary.**

```bash
target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"; echo "exit=$?"
cat "$X/bench.txt"
grep -cE ' FAIL$' "$X/bench.txt"; grep -cE ' PASS$' "$X/bench.txt"
cargo test -p talaria-bench --test real_binary release_budgets_hold -- --ignored 2>&1 | grep 'test result'
```

PASS: `exit=0`; the FAIL count is 0; the table has a PASS row for each of the budgets M1 to M7 of P1-11's table (its specification section 3; rows are named by what they measure, such as `launch to first frame`; M8, the live gateway row, is SKIPPED by design and is neither); and the ignored test's line reads `test result: ok. 1 passed` (a renamed test matches nothing and Cargo then exits 0 with `0 passed`, which is a FAIL). Evidence: the table from the `os: ` line to the last row, the two counts, the exit line, the `test result` line. The architecture budgets are in [05-architecture.md](../05-architecture.md#performance-budgets).

**AUTO-4: live suites with no skip.**

```bash
cargo test -p talaria-tui --test tmux_scrollback -- --ignored --nocapture
cargo test -p talaria-tui --test composer_pty -- --nocapture
cargo test -p talaria-tui --test composer_pty -- --ignored --nocapture
for t in new_ask_quit_resume_roundtrip quit_mid_turn_leaves_the_turn_running continue_in_a_fresh_folder_is_refused approval_card_answers_rm_rf_pytest_cache launch_to_usable_composer_within_budget interrupt_and_resume_list; do
  TALARIA_LIVE_REQUIRE=1 cargo test --release -p talaria-cli --test live_session -- --ignored --nocapture --exact "$t" > "$X/live-$t.txt" 2>&1; echo "$t exit=$?"
  grep -E 'LIVE_SESSION_RESULT|^test result' "$X/live-$t.txt"
done
```

PASS: exactly one line `TMUX_SCROLLBACK_RESULT: PASSED`; neither composer run prints `SKIP:`; each of the six tests of the loop (`P1-12` tests 40 to 45, run one at a time by exact name because the result line carries no test name) prints `exit=0`, exactly one `LIVE_SESSION_RESULT: PASSED` and `test result: ok. 1 passed`, and no `SKIPPED` (so six lines in all; a name that matches no test prints `0 passed` and is a FAIL). A skip because the gateway's code commit differs from the pinned one, or a missing precondition, is `BLOCKED (environment)`, never a pass. Evidence: the result lines, with each live test named.

**AUTO-5: hygiene scans.**

```bash
cargo test -p patterns-check
! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools
! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input.rs' ':!tools/smoke/README.md'
! git grep -nE 'end_reason|unattended|config\.set\b' -- crates/talaria-app/src crates/talaria-cli/src
! git grep -nE '"unattended"|unattended: *(true|Some)' -- crates/hermes-client | grep -vE ':[0-9]+: *//'
! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile'
```

PASS: every command exits 0 and the `!` lines print nothing. The wire rules these scans enforce are the ones `P1-12` and `P1-13` state: Talaria never sends `end_reason` on `/quit`, never edits Hermes configuration (`config.set`) and never sends `unattended`; the smoke suite never mentions `unattended` outside its own test and README. Scope (ruling R-5): `crates/talaria-app/src` and `crates/talaria-cli/src` are the folders where `P1-12` forbids the three words even in comments (its tests that name them live in `tests/`, which is not scanned); the wire client lives in `crates/hermes-client`, where `P1-03`'s own rule is the `"unattended"` pattern with comment lines excluded, so that is the scan used there; `end_reason` and `config.set` are not scanned in `hermes-client` because `P1-04`'s redaction rules name `config.set` there. The generated `crates/hermes-contract` holds every field name and is not scanned; `P0-03`'s tests cover its `unattended` default. The behaviour is covered live by `quit_mid_turn_leaves_the_turn_running` in AUTO-4, which proves a quit ends no session. A word inside a comment in the two P1-12 folders is a FAIL, reported to the owner, who rewords it. The unsafe scan covers the whole tree here (the source issues scanned their own folders); any finding is a FAIL, reported on this issue, and a new candidate follows. A `unsafe` that is legitimate in generated code outside the product crates is a finding against the P0-04 rule: the lead reports it on the owner's issue, and the row stays FAIL until the rule or the code changes and a new candidate passes. The profile scan matches whole names only. Evidence: each command with `exit=0`.

**AUTO-6: smoke suite offline tests.**

```bash
cargo test -p smoke
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
cargo run -q -p smoke -- --help
cargo run -q -p smoke -- run --help
```

PASS: all tests pass; the ignored list is exactly one line and names `live_full_smoke_in_herdr`; the help of `run` lists `--talaria`, `--herdr`, `--profile`, `--model`, `--provider`, `--deadline-secs`, `--step-scale` and `--report-dir` (if `P1-13` placed some of them on the top-level help instead, the two helps together list them all). Evidence: the `test result` line, the ignored line and both helps.

**P1-13 UAT-1 then AUTO-7: the smoke suite twice in a row.** These are the two live runs of the smoke suite and they run back to back at the end of stage 1, with no other live scenario between them. P1-13 UAT-1 is the first run as `P1-13` writes it (with `--talaria target/release/talaria`, rule REQ-6); AUTO-7 is the second:

```bash
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria target/release/talaria > "$X/smoke2.txt" 2>&1; echo "exit=$?"
cat "$X/smoke2.txt"
awk '/^SMOKE_RESULT: PASSED/ { split($0, a, " "); if (a[4] + 0 < 300) ok = 1 } END { print ok + 0 }' "$X/smoke2.txt"
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
```

PASS: six `PASS` step lines, `SMOKE_RESULT: PASSED (elapsed <n> s)` with the awk line printing 1 (n under 300), `exit=0`, and no `talaria-smoke-` tab in the second list. The suite prints `PASSED WITH SKIPS` and exits 3 when its approval step is skipped, and with `TALARIA_LIVE_REQUIRE=1` a skip fails the run; neither is a pass here. The smoke suite passes `--no-ensure` to `talaria` itself (`P1-13` REQ-4 and REQ-10), so it can never start a gateway. Evidence: the whole output and both tab lists; each of the two evidence sections starts with its own `SMOKE_RESULT:` line inside its block.

### 7.2 Source rows (49 scenarios)

Run each as written in Appendix A, with the four allowed changes of the issue's REQ-6:

1. `target/release/talaria` (the absolute `$T`) wherever a scenario says `talaria`, `<talaria>` or the debug path.
2. `--no-ensure` on every launch of `talaria` that starts a live session and lacks it. A launch that only replays a recording (`--bench-replay`) never connects and takes none, and the launches `talaria-bench` makes itself are not scenario launches.
3. P1-11 UAT-5 (the operator's macOS run) uses `--ref p1-acc-candidate`, replacing the source's `--ref p1-11-performance-harness` (or adding it where the source omits `--ref` once merged), and its evidence includes `gh run view <id> --json headSha --jq .headSha` equal to the candidate.
4. Every `smoke run` that lacks `--talaria` gets `--talaria target/release/talaria`. The smoke program itself keeps its debug build (`cargo run -p smoke`, and `target/debug/smoke` in P1-13 UAT-4); it is a tool, not the product, and it passes `--no-ensure` to `talaria` on its own.

These are not changes: cleanup before a scenario starts (kill a leftover tmux session of the same fixed name such as `p106`, `p108` or `accxf3`, with `tmux kill-session -t <name> 2>/dev/null`; close a leftover `talaria-smoke-` tab); a key name Herdr rejects sent with `tmux send-keys` where the source scenario itself allows it (P1-08 UAT-1; say so in the evidence); the pane-split form the source scenario writes. A test target or demo that no longer exists is `BLOCKED (not built)`.

**Helper commands of the two rows from phase 1a (ruling R-6).** `P1-03` UAT-1's `reconnect_watch` is the example of section 4.7 of `P1-03`'s unit specification ([P1-03.md](P1-03.md#47-the-example-reconnect_watch)); its precondition `cargo build -p hermes-client --examples` is a debug build, which the scenario's own `cargo run` also makes. It discovers the gateway with `EnsurePolicy::Never`, so it cannot start one and takes no `--no-ensure`. `P1-05` UAT-3's "P1-04's script" is `tools/record-fixture`: `LIVE=$(mktemp -d); cargo run -q -p record-fixture -- record --out "$LIVE" --profile fixture --model minimax-m3.1-flash --provider ollama-cloud` (the flags of `P1-04` REQ-17; check them with `--help` at the candidate). It also uses `EnsurePolicy::Never`, and it exits 2 with a plain message when a precondition is absent: no gateway, the `fixture` profile not served, a gateway code commit that differs from the pin, `approvals.mode` not `manual`, or a login or host name shorter than four characters; exit 2 is `BLOCKED (environment)`. Then the row's own `live_reduce` command runs on `$LIVE`. The no-ensure rule covers every launch of `talaria`, including `-c` and `--resume` with no id (the list picker of `P1-12` UAT-8); `smoke run` passes it to `talaria` itself.

**Quitting between rows (ruling R-9).** Every row that starts `talaria` ends with `/quit` and waits for `Resume this session with:` before the next row starts, unless the source row says to leave it open; so the session `P1-12` UAT-2 resumed is quit before UAT-3 starts, and each row's evidence ends with that line.

Order within an issue is the scenario number order. Dependencies between scenarios: P1-12 UAT-2 needs UAT-1's folder family; UAT-3 continues UAT-2's session; UAT-4 uses a new empty folder; UAT-6 and UAT-7 need `approvals.mode: manual`. UAT-8 needs a session whose long turn was quit while working, and UAT-2's session cannot be that (it was resumed after 60 s, and its prompt has 400 numbers): immediately before UAT-8, make its Given true with UAT-2's own steps in a new folder with the 1000-number prompt, quit while the status line shows working, do not wait and do not resume, then run UAT-8's steps at once; the evidence says so. P1-13 UAT-5 changes the profile and runs last of all rows (stage 3); the operator logs any gateway restart they make for it and the read-back of `approvals.mode: manual` in the evidence. Rows of the same issue that each need a clean pane use a new pane per scenario.

The persona and runner of each row are in Appendix A. Operator rows among them: P1-06 UAT-4, P1-07 UAT-6, P1-08 UAT-5, P1-11 UAT-5, P1-13 UAT-5. P1-12 UAT-7 (the 30-minute session) is run by the agent tester in stage 2. P1-13 UAT-1 runs in stage 1 as the first of the two smoke runs (section 7.1).

### 7.3 Cross-feature rows XF-1 to XF-8 (persona developer; agent tester in Herdr)

These need the live gateway, profile `fixture`, and the common settings of section 3 (every pane starts with `. ../acc.env`). Each uses its own folder `P=$(mktemp -d)` and its own panes, each created with `PANE=$(pane_new "$P")` (a second pane `PANE2=$(pane_new "$P")` where a row says so). "Start Talaria in `$PANE`" means `herdr pane run "$PANE" "$T $F"` and waiting until `herdr pane read "$PANE" --source visible` shows `minimax-m3.1-flash`. Before starting Talaria in a pane, type `echo READ-CHECK` there and confirm which of `visible` and `recent-unwrapped` shows it; use that source for the row (section 2.1: a read that lacks text just typed is stale and never evidence). The read check applies to every Herdr pane, including the second pane of XF-4, XF-6 and XF-8. XF-3 reads through `tmux capture-pane`, which asks tmux for its own screen, so it needs no read check; it waits for the model name before it types. Prompts are typed with `herdr pane run "$PANE" '<text>'`; if after 8 s nothing happened (a still screen, no counting line, no card), send `herdr pane send-keys "$PANE" enter` once and say so in the evidence (the same fallback as P1-13). Counting helper (reads a pane text file on standard input; the same line rule as P1-13 section 5.5):

```bash
count_check() { python3 -c '
import re, sys
n = int(sys.argv[1]); seq = []
for line in sys.stdin:
    m = re.fullmatch(r"[\W_]{0,6}(\d{1,4})[.)]?\s*", line.rstrip("\n"))
    if m and 1 <= int(m.group(1)) <= n: seq.append(int(m.group(1)))
print("counting_lines=%d highest=%s consecutive_from_1=%s" % (len(seq), max(seq) if seq else None, seq == list(range(1, len(seq) + 1))))
' "$1"; }
```

`consecutive_from_1=True` with `highest` equal to the requested number means every number from 1 to N appeared once, in order. A model that miscounts makes a run fail without being a Talaria defect: rerun once, record both runs, and keep a failing second run as FAIL (ruling R-3). What a missing card or a too-fast reply means for each row is in ruling R-7.

**XF-1: Type while a long reply streams.** Covers the inline renderer, Markdown, composer, status line and gateway together.
- Preconditions: operator sets up the live preconditions.
- Steps: `PANE=$(pane_new "$P")`; `herdr pane run "$PANE" "$T $F"`; wait until `herdr pane read "$PANE" --source visible` shows `minimax-m3.1-flash`. `herdr pane run "$PANE" 'Count from 1 to 1000, one number per line, then write the sum of 6000 and 1731 on the last line.'` Poll `visible` every second until a counting line `20` is shown. Within 2 seconds run `herdr pane send-text "$PANE" 'draft-'`, `sleep 0.3`, `... 'one-'`, `sleep 0.3`, `... 'two-'`, `sleep 0.3`, `... 'three'` (never Enter). Read `visible` once right after the last piece and note the highest counting line (it must be below 900, else the reply was too fast and the run is void: rerun, counted as a run, with 3000 numbers, where `count_check 3000` and `highest=3000` replace the 1000 figures below). Wait for `7731` (`herdr pane wait-output "$PANE" --match 7731 --timeout 240000`), then wait until two `visible` reads 3 s apart are identical. Capture `herdr pane read "$PANE" --source visible > "$X/vis.txt"` and `herdr pane read "$PANE" --source recent-unwrapped --lines 5000 > "$X/rec.txt"`. Then `herdr pane send-keys "$PANE" ctrl+c`, `herdr pane run "$PANE" '/quit'` and wait for `Resume this session with:`.
- Pass: `count_check 1000 < "$X/rec.txt"` prints `highest=1000 consecutive_from_1=True`; `grep -c 'draft-one-two-three' "$X/rec.txt"` prints 1 (the composer line only: a draft sent by accident would appear a second time as a prompt line); `grep -c 'draft-one-two-three' "$X/vis.txt"` prints 1; `grep -cw 7731 "$X/rec.txt"` prints 1 or more; the two counts of 1 are the whole check that nothing was sent.
- Evidence: the `count_check` line, the three `grep` counts, the last 15 lines of `vis.txt`, the highest counting line when typing started.

**XF-2: Esc during a reply stops it and keeps the draft.**
- Steps: new folder and pane; start Talaria as in XF-1. `herdr pane run "$PANE" 'Count from 1 to 2000, one number per line, and do not stop early.'` Wait for a counting line `15`. `herdr pane send-text "$PANE" 'keepdraft'`; `herdr pane send-keys "$PANE" esc`; `sleep 0.4`. Poll up to 20 s until two `visible` reads 3 s apart are identical; keep the second as `$X/vis1.txt` and `recent-unwrapped` as `$X/rec1.txt`. `herdr pane send-keys "$PANE" ctrl+c`; read `visible` as `$X/vis2.txt`. `herdr pane run "$PANE" 'Reply with only the word OK.'`; wait until a line holding only `OK` appears after the echo, then `herdr pane read "$PANE" --source recent-unwrapped --lines 5000 > "$X/rec2.txt"`; `herdr pane run "$PANE" '/quit'`. If at the moment of Esc the last `visible` read shows a counting line above 1899, or the reply has ended, the run is void (too fast to show that Esc stopped it): rerun, counted, with `Count from 1 to 6000, one number per line, and do not stop early.`, `count_check 6000` and `highest` between 15 and 5899.
- Pass: the screen settled within 20 s of Esc; `count_check 2000 < "$X/rec1.txt"` shows `consecutive_from_1=True` and `highest` between 15 and 1899 (the partial reply stays, and the reply stopped early); `grep -c keepdraft "$X/vis1.txt"` prints 1; `grep -c keepdraft "$X/vis2.txt"` prints 0 (Ctrl+C clears a draft: P1-08 settled decision 1); a line that is `OK` alone appears (`grep -cE '^[^A-Za-z0-9]*OK\.?[^A-Za-z0-9]*$'` prints 1 or more on `$X/rec2.txt`).
- Evidence: the four reads' key lines, the `count_check` line, the greps, the `OK` line from `$X/rec2.txt`. Cleanup: `/quit` printed the resume lines.

**XF-3: Resize a tmux window twice during a real reply.**
- Preconditions: `tmux` installed; live preconditions.
- Steps: `S=accxf3`; `tmux kill-session -t "$S" 2>/dev/null`; `tmux new-session -d -s "$S" -x 100 -y 30 -c "$P" "$T $F"`; poll `tmux capture-pane -p -t "$S"` until it shows `minimax-m3.1-flash`; `B0=$(tmux capture-pane -p -S - -t "$S" | grep -c minimax-m3.1-flash)`. `tmux send-keys -t "$S" 'Count from 1 to 300, one number per line, then write the sum of 6000 and 1731 on the last line.'`; `sleep 1`; `tmux send-keys -t "$S" Enter`. When a counting line of 30 or more is on the screen (poll `tmux capture-pane -p -t "$S"` every 0.3 s for a line holding only a number from 30 to 250, so that a fast reply cannot scroll past it): `tmux resize-window -t "$S" -x 80 -y 20`; `sleep 2`; `tmux resize-window -t "$S" -x 100 -y 40`. Wait for `7731` and for two captures 3 s apart to be identical. `tmux capture-pane -p -S - -t "$S" > "$X/xf3.txt"`. Then `tmux send-keys -t "$S" '/quit'`, `sleep 1`, `tmux send-keys -t "$S" Enter`, `sleep 2`, `tmux kill-session -t "$S"` (ignore "no such session": quit ends the session).
- Pass: `count_check 300 < "$X/xf3.txt"` prints `highest=300 consecutive_from_1=True`; `grep -c minimax-m3.1-flash "$X/xf3.txt"` equals `B0` (this assumes the model name appears only in the status line; a left-behind copy of the live region would add one); `7731` appears.
- Evidence: the `count_check` line, `B0` and the final count, `tmux -V`.

**XF-4: An approval card arrives while the composer holds a draft.**
- Preconditions: operator sets `approvals.mode: manual` on `fixture`; else BLOCKED (environment).
- Steps: `mkdir "$P/.pytest_cache" && touch "$P/.pytest_cache/x"`; panes `$PANE` (Talaria) and `$PANE2` (a shell, both from `pane_new "$P"`). Start Talaria in `$PANE`. `herdr pane run "$PANE" 'Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache'`; at once `herdr pane send-text "$PANE" 'note-to-self'`. Poll `visible` up to 90 s until it holds `rm -rf .pytest_cache` and the lines `1. once` and `4. deny` (the numbered choices of the card, as P1-09 UAT-1 reads them; the typed prompt and the reply do not contain them); read A. `herdr pane send-keys "$PANE" 1`; read B; `herdr pane run "$PANE2" 'ls -a'` and read it (list 1). `herdr pane send-keys "$PANE" backspace backspace backspace backspace backspace backspace backspace backspace backspace backspace backspace backspace backspace` (13: the 12 characters and the stray `1`); `sleep 1`; read C. `herdr pane send-keys "$PANE" 1`; wait up to 60 s for the reply and a settled screen; read D; `herdr pane run "$PANE2" 'ls -a'` (list 2). `/quit`.
- Pass: read A shows the card and a hint line containing the word `draft` (record the exact text; P1-09 REQ-15 requires a hint while the composer holds text), and the composer holds `note-to-self`; B still shows the card, and the composer shows `note-to-self1`; list 1 still shows `.pytest_cache`; C shows the composer empty and no hint containing `draft`; D shows no card; list 2 has no `.pytest_cache`; the agent's reply in D contains the phrase `exit code 0` (any case).
- Evidence: reads A to D (key lines), both listings, the exact hint text.

**XF-5: The agent asks a clarifying question.**
- Steps: new folder and pane, start Talaria. `herdr pane run "$PANE" 'Use your clarify tool, not plain text, to ask me which colour I prefer, offering exactly the choices red and blue. After I answer, reply with only: you chose <my answer>.'` Poll `visible` up to 90 s for a card showing both `red` and `blue`. Read the first highlighted line (starts with `›`, `>` or `❯`) and note which choice it holds, `<c>`. `herdr pane send-keys "$PANE" enter`; wait for the reply; read. `/quit`. If no card appears, retry the sentence twice with `You must call the clarify tool.` added (the only permitted change to a prompt in this issue; no other prompt is ever eased); after three attempts without a card the row is `BLOCKED (environment)` (the free model did not use the tool) and the operator may rerun it by hand with a stronger model (`--model` and `--provider` of their choice, written in the evidence), and an agent copies the operator's result and comment URL into the row; if that rerun meets the pass rule the row is PASS, and its evidence names the model. The three-attempt failure of the free model on this row is not a stop condition.
- Pass: a card with `red` and `blue` appeared; after Enter it closed within 2 s; a line containing `you chose <c>` appears in the reply (case ignored, a full stop ignored; `<c>` is the text of the highlighted choice, `red` or `blue`); no card remains.
- Evidence: the card read, the highlighted line, the reply read.

**XF-6: Resume shows the whole earlier conversation once.**
- Preconditions: as XF-4 (approval needed).
- Steps: `N=$((RANDOM % 9000 + 1000))`; `mkdir "$P/.pytest_cache"`; start Talaria in `$PANE`. Submit `Reply with a Markdown heading "Plan", two bullet points, and a Python code block that prints hello. Then write MARK-$N alone on the last line.`; wait until `grep -cw "MARK-$N"` of the pane text is 2 or more and the screen settles. Submit the approval prompt of XF-4 (without typing a draft); when the card shows, `sleep 1`, press `1`, wait for `.pytest_cache` to be gone. `K1=$(herdr pane read "$PANE" --source recent-unwrapped --lines 3000 | grep -cw "MARK-$N")`, `Q1=$(... | grep -c 'tell me its exit code')`. `/quit`; note the printed id. `PANE2=$(pane_new "$P")`; `herdr pane run "$PANE2" "$T --no-ensure -c --profile fixture"` (no `--model` or `--provider`: Talaria refuses them with `-c`, and the status line must show the model recorded in the session); wait for a line starting `── Resumed:`. `K2` and `Q2` the same greps on `$PANE2`. Submit `Reply with only the word OK.` in `$PANE2`, wait for the answer, `/quit`, note the id.
- Pass: `grep -c '^── Resumed:'` of the `$PANE2` read prints 1; `K2` equals `K1` and is 2 or more; `Q2` equals `Q1` (and `Q1` is 1); both ids are equal; the `visible` read of `$PANE2` right after the resume shows `minimax-m3.1-flash`; `grep -cE '^[^A-Za-z0-9]*OK\.?[^A-Za-z0-9]*$'` of the final `$PANE2` read prints 1 (`OK` answered once).
- Evidence: `K1 K2 Q1 Q2`, the ids, the `Resumed` line, the last 15 lines of each pane.

**XF-7: Idle cost on the real binary.**
- Steps: new folder and pane; start Talaria in `$PANE`. After 5 s at an empty composer: `PID=$(herdr pane process-info --pane "$PANE" | jq -r '.result.process_info.foreground_processes[] | select(.cmdline | contains("target/release/talaria")) | .pid')`; sample 0: `R0=$(ps -o rss= -p "$PID")`. Submit `Reply with only the word OK.`, wait for the answer and 5 s more. Sample 1: `R1`, `C1` (`ps -o time= -p "$PID"`), `V1` (`herdr pane read "$PANE" --source visible`). `sleep 120`. Sample 2: `R2`, `C2`, `V2`. Convert `time=` to seconds with `awk -F: '{ s = 0; for (i = 1; i <= NF; i++) s = s * 60 + $i; print s }'`.
- Pass: `R0` is at most 40960 (KiB, the 40 mebibyte start budget; P1-11 decision 7 reads MB as mebibytes); `R2` is at most 102400 (the 100 mebibyte budget); `R2 - R1` at most 4096 (this issue's check for idle growth, not an architecture budget); `C2 - C1` at most 1 second (about 0.8 percent of one core; ps resolution is coarse); `V1` and `V2` are identical. The 40 limit is the architecture's budget at start (`05-architecture.md`, performance budgets). The 100 limit is the architecture's budget after a 2,000-message session; after one reply and two idle minutes it is only a sanity ceiling, so a pass at 100 says nothing about that budget (AUTO-3 owns it). The growth limit of 4096 KiB and the CPU limit of 1 second are this issue's own sanity checks for two idle minutes, not architecture budgets; the operator confirms or changes them on the parent issue before the candidate is frozen. The `jq` path above is the shape the P1-13 issue reads from `herdr pane process-info` (verify it on the Herdr in use); a `ps` or `jq` that finds no process makes the row BLOCKED (environment). Two idle screens that differ for any reason (a clock, a spinner) are a FAIL: attach both.
- Evidence: the three samples and the two conversions, the identical `V1`/`V2` check (`cmp`).

**XF-8: Ctrl+C with an approval card open (negative).**
- Preconditions: as XF-4.
- Steps: as XF-4 up to the card (panes, folder and Talaria start included) but type no draft. When the card shows, `sleep 1`; `herdr pane send-keys "$PANE" ctrl+c`; poll `visible` up to 5 s until neither `1. once` nor `4. deny` is shown (the card words, as in XF-4); `herdr pane run "$PANE2" 'ls -a'` (list); wait until two `visible` reads of `$PANE` 3 s apart are identical (30 s limit) and keep the second as read E. `herdr pane run "$PANE" 'Reply with only the word OK.'`; wait for a line `OK` alone.
- Pass: no card within 5 s; the command does not run (the listing still has `.pytest_cache`); read E has no `stopping` and its status line does not show `working` (the turn stopped); the second prompt is answered `OK`.
- Evidence: reads, listing.

### 7.4 Operator rows OP-1 to OP-6

The operator uses `target/release/talaria` of the candidate for a working day inside Herdr (at least four hours of real coding on the first candidate; one hour on a later one, same counts). The repository and branch are the operator's choice (a throwaway branch). Any model and provider the operator likes; the record writes the profile as `default` or `fixture` and the model by its public id. `approvals.mode` must be `manual` for OP-1; the operator sets it up (agents never do). Start every session with `--no-ensure` unless the operator wants Talaria to start the gateway on purpose, in which case the evidence says so. The operator may provoke an approval card or a clarifying question at any time with the prompts of XF-4 and XF-5, and says so in the day log. The operator builds nothing: the lead tells them (outside the issue) where the candidate binary is, and the day log gives the SHA-256 of the binary used, which must equal the one in the AUTO-1 evidence.

The operator posts one comment on the issue per row OP-1 to OP-5 with its counts and result, and the verdict comment (below) for OP-6; an agent copies each result and its notes into the evidence section, scrubbed of identifiers only (section 5.3, ruling R-2), and cites that comment's URL. The operator, not an agent, decides a result. The day log (row OP-6) holds: start and end time and hours; the counts of OP-1 to OP-5; each defect with its issue number; the candidate commit and the SHA-256 of the binary; any gateway restart or Hermes configuration change the operator made and the read-back of `approvals.mode`.

| Row | Counts to keep | PASS only if |
|---|---|---|
| OP-1 | cards seen, answered once, answered session or always, denied; questions answered | at least 3 approval cards answered, at least one `deny` and one `once`; at least 1 question answered; for each answer the next agent line reflects it; zero cards left on screen after an answer; zero answers that went to a different choice than the key pressed |
| OP-2 | replies of 30 lines or more (the reply's own lines; a wrapped line counts once) during which the operator typed 20 or more characters; per episode: all characters shown, any lag seen | at least 5 episodes; in each all characters appear as typed (the operator types a string they know and compares the composer with it after the reply) and the draft is intact; no episode with a lag the operator calls noticeable. The benchmark table of AUTO-3 is attached for the numbers; the operator's word decides the feel |
| OP-3 | per terminal (the host terminal, tmux if used, the plain terminal): result of search, copy and scroll | after at least 300 lines of scrollback: the terminal's own search finds a distinctive word of the first reply; a code block of 5 or more lines copied with the terminal's selection pastes into a file that has no box-drawing characters (U+2500 to U+257F) or gutter characters and passes `bash -n` (for shell) or compiles (for other languages); scrolling back reaches the first prompt, shown once. The operator asks the agent for the code block and writes the searched word in the evidence |
| OP-4 | each quit and resume: how (`--resume <id>`, `-c`, the `--resume` list), what was running at quit | at least 3, one of them with an approval card open at quit (the XF-4 prompt; resume within 2 minutes; the card is shown again and takes an answer; ruling R-11); after each resume the last five prompts and their answers are present once (older history may be summarised by the line `(<n> earlier messages not shown)`, which `P1-12` REQ-12 prints when more than 50 messages exist), nothing is duplicated, the status line shows the right model, and a reply that finished while Talaria was closed is complete |
| OP-5 | occurrences by kind, each with a screenshot and an issue number | zero repeated lines, zero status line still showing `working` more than 5 s after a reply ends, zero wrong model, zero raw escape text (`^[`, `[2J`) shown as text, zero cards left after answering, zero broken box borders; checked after every resize (at least 3), list picker, approval card and Ctrl+C |
| OP-6 | the day log (above) | at least 4 hours (1 on a later candidate); all counts present; every defect has an issue (filed by the operator, or by the lead at the operator's request, as a new issue for the source issue's owner); the comment with a line reading exactly `PHASE 1 OPERATOR CHECK: PASSED (candidate <7 hex>)` is posted by the operator at the end of stage 3. The same comment holds the day log and lists the terminals and multiplexers used, and may carry lines `ACCEPTED GAP #<n>` for a cosmetic defect that violates no OP row (ruling R-4) |

The terminal matrices of P1-06 UAT-4, P1-07 UAT-6 and P1-08 UAT-5 are done once in the operator's terminals at the start of stage 3. The operator writes in the verdict comment the list of terminals and multiplexers used, so OP-3 is judged in each.

## 8. Run order

1. Stage 0: freeze and build (section 3), the preflight, the `acc.env` file. The operator confirms or changes the two XF-7 idle limits and the shape of this record on the parent issue. Commit the failing tests and the stub (test author, `test(acceptance): add the record checker tests and a stub`), then the empty record (`docs(acceptance): add the empty record`; its red run is made once the checker exists), then the checker (`docs(acceptance): add the record checker`).
2. Stage 1: AUTO-1 to AUTO-6, then the two smoke runs back to back (`P1-13 UAT-1`, then AUTO-7). A FAIL ends the run: report, fix, new candidate (ruling R-8); a BLOCKED row ends nothing and the next row runs.
3. Stage 2: source rows (`P1-03 UAT-1`, `P1-05 UAT-3`, then `P1-06` to `P1-13` in issue order, skipping the operator rows and `P1-13 UAT-1`), then XF-1 to XF-8. One live scenario at a time. Three identical provider errors in a row (the same HTTP status and the same error message text) stop the run (a standing answer, not a wait). A FAIL does not stop the other rows of stage 2; stage 3 does not start until a candidate has no FAIL in stages 1 and 2.
4. Stage 3 (operator): the terminal matrices (P1-06 UAT-4, P1-07 UAT-6, P1-08 UAT-5), `P1-11 UAT-5` (the macOS workflow run, `--ref p1-acc-candidate`), the day (OP-1 to OP-6), then `P1-13 UAT-5` last, restoring `approvals.mode: manual` and reading it back. The operator then posts the verdict comment.
5. Stage 4: an agent copies the operator's results and comment URLs into the record, fills the rest, runs `check-record.sh` and opens the pull request (documentation only) with the `UAT-<n>: PASS` lines, the `Defects found:` line and the `Learnings:` line. The lead merges it when the checker passes and the verdict comment exists (AC-8), then deletes the branch `p1-acc-candidate` and the worktree.

Expected duration is not measured here; the operator's day is the long part.

## 9. Smoke suite: extension design

**Decision: this unit extends the smoke suite by nothing.** It runs `smoke run` as P1-13 builds it, twice, on the release binary. No file under `tools/smoke` changes. Reasons: P1-13 owns that crate and a second writer would collide; the smoke suite is meant to stay small and under its 300 second deadline, and every step added takes time from that; and the behaviours below need the judgement of a tester or the operator for now.

Coverage map of the smoke suite against this acceptance:

| Behaviour | Smoke step | Where else it is checked |
|---|---|---|
| plain answer, token appears | `plain-answer` | P1-12 UAT-1, XF-1 |
| approval answered once, effect on disk | `approval` | P1-12 UAT-6, XF-4, XF-6, OP-1 |
| interrupt keeps the session alive | `interrupt` | XF-2, XF-8, P1-08 UAT-3 |
| quit exits 0, history stays in scrollback | `quit` | P1-12 UAT-1 and UAT-2 |
| resume shows history once, new prompt works | `resume` | P1-12 UAT-3, XF-6, OP-4 |
| type while streaming; draft kept | none | XF-1, XF-2, OP-2 |
| deny path; card while a draft exists | none | XF-4, XF-8 |
| clarifying questions | none | XF-5, OP-1 |
| resize while streaming | none | XF-3, P1-06 UAT-2 |
| native search and copy | none (scrollback tokens only) | OP-3 |
| idle cost | none | XF-7, AUTO-3 |

Design for adding a step later (for example the phase 2 acceptance, which extends the smoke suite to every phase 2 row). A proposal, to be re-verified against the smoke code that exists by then.

- A step is a function in `tools/smoke/src/steps.rs` that takes the `Terminal` and `Clock` traits (so it is testable against `fake-herdr` in milliseconds), the config and the pane ids, and returns `StepOutcome`. It uses the pure helpers of `screen.rs` (the card rule, counting lines, `settled`) and adds a helper there only with its own test.
- It adds its exact FAIL reasons to the reason table of P1-13's specification section 4.10, a line to the README section `## Steps`, a recorded fixture to `tests/fixtures/herdr/` for the offline test, and keeps the whole run under the 300 second deadline (re-measure the elapsed time of the five existing steps before adding time).
- Candidates, in order of value: `typed-while-streaming` (XF-1: type a draft during a counting reply, require 1 to N consecutive lines and the draft in the composer after); `esc-keeps-draft` (XF-2: the same with Esc, pass rule as in REQ-8 of P1-13 for M0, M1 and M2 plus the draft); `deny-path` (the approval step answered `deny`, require the directory still there and the session alive). Each reuses the counting-line and settled rules of P1-13 sections 5.5 and 5.6, which is why they are the cheapest to automate.
- Not candidates: native search and copy (they belong to the terminal, not to Herdr's pane reads), the clarifying question (depends on the model calling a tool), and idle cost (a `ps` reading; belongs to the benchmark harness of P1-11).

## 10. Failure handling

| Situation | Rule |
|---|---|
| A row fails | Record FAIL with the scrubbed pane text; comment on the issue; the lead files a defect; the fix merges; new candidate; rerun everything. |
| A row fails for a reason outside Talaria (provider error, model miscount) | Rerun, at most three runs per row; every run is in the evidence with its cause; the last run decides. Three identical provider errors in a row stop the run. |
| A needed precondition is absent | `BLOCKED (environment)`; ask the operator; the row is not a pass until it is run. |
| A demo or test named in a source issue is gone | `BLOCKED (not built)`; report on the issue of the owner; not a pass. |
| A source scenario's text differs from Appendix A | The source issue's text wins; note the difference in the evidence. |
| The operator finds a defect in the day | Screenshot, comment, issue (filed by the operator, or by the lead at their request, for the source issue's owner); continue the day; the fix creates a new candidate. The lead files issues for defects the agents find. |
| The candidate must change | Every row is run again on the new candidate (the checker refuses a commit that is not the candidate). The record branch is rebased onto it; the old record stays in git history. For the six operator rows a session of at least one hour meeting the counts of section 7.4 is enough (the operator may provoke cards and questions); the terminal matrices are repeated in full. |
| A row passed earlier on a commit that is not the candidate | It is not a pass here (the checker refuses it). |

### 10.1 Rulings on edge cases

These answer questions a builder asked when planning from the issue. They bind like the issue; where one differs from the issue's text, the issue is edited to match.

- **R-1 Long captures.** A section may hold several blocks of 60 lines or fewer and may excerpt a long read by the rule of section 5.1 (the checking commands, the first 15 and last 15 lines, every line the Then clause names, cuts marked). The source rows that ask for a whole read (`P1-06` UAT-1's 400-line read, `P1-06` UAT-2's capture, `P1-11` UAT-3's two reads, `P1-12` UAT-7's scrollback) are shown this way.
- **R-2 Operator comments.** An agent copies the operator's counts, results and words, and replaces only identifiers by the scrub of section 5.3, saying so on the row (`Scrubbed: ...`). It never changes a number or a verdict word. A secret makes the agent ask the operator to edit the comment; nothing is copied until then.
- **R-3 Runs, and who decides.** A run is one execution of a row's steps, from its first step to its result. A *void* run ended before the behaviour under test could be seen, for a cause outside Talaria that the section names: a provider error shown in the pane (HTTP status and message), the free model not calling the tool it was asked to use (XF-4, XF-5, XF-6, XF-8), or a reply too fast for the row's guard (XF-1, XF-2). A void run is listed and counts toward the three-run limit, but it is not a failure. A run that reached the behaviour and got the wrong result is FAIL at once and is not rerun, except for a model miscount in a counting row (XF-1, XF-2, XF-3, XF-6), which allows exactly one rerun (a failing second run is FAIL). The last run decides. XF-5: the three attempts, with or without the added sentence, are runs 1 to 3 (void when no card comes); the operator's rerun with a stronger model is the one run allowed beyond three (`Run 4 of 4`, runner `operator`). The XF-1 variant with 3000 numbers is a run like any other. The lead names a reviewer on the pull request who did not run the row; for every row with more than one run the reviewer writes on the pull request that each earlier run was void or a miscount, citing the evidence lines. A reviewer who disagrees makes the row FAIL and the lead files the defect.
- **R-4 OP-6 and the day log.** The verdict comment is OP-6's comment and holds the day log. In one comment, in this order: the line `PHASE 1 OPERATOR CHECK: PASSED (candidate <7 hex>)`, the day log (times and hours, the counts of OP-1 to OP-5, each defect with its issue number, the candidate commit, the binary's `shasum -a 256`, any gateway restart or configuration change with the read-back of `approvals.mode`), the terminals and multiplexers used, and any `ACCEPTED GAP #<n>` lines. When a row is not PASS the operator posts the day log without the verdict line. An agent copies the comment into the OP-6 section (scrubbed, R-2) in blocks of 60 lines or fewer and cites its URL.
- **R-5 AUTO-5 scope.** As in AUTO-5 above: the two folders where `P1-12` forbids the three words, the `"unattended"` pattern in `crates/hermes-client`, the live cover of AUTO-4. A comment in the two folders counts as a hit.
- **R-6 Helper commands and `--no-ensure`.** As in section 7.2: `reconnect_watch` is section 4.7 of `P1-03`'s unit specification, the recorder is `tools/record-fixture` with the flags shown there, neither can start a gateway, and every launch of `talaria` carries `--no-ensure`.
- **R-7 When the expected screen does not come.**

| Situation | Outcome |
|---|---|
| XF-4, XF-6, XF-8: no card within 90 s and the command did not run (`.pytest_cache` still listed) | void run (the model did not call the shell tool); rerun, at most three runs, then `BLOCKED (environment)` |
| the same, but the command ran (folder gone) and the operator's read-back of `approvals.mode: manual` for this candidate is on the issue | FAIL (a dangerous command ran without a card) |
| the same, with no such read-back | `BLOCKED (environment)` |
| XF-4, XF-6, XF-8: a card is still shown 5 s after Ctrl+C (XF-8), after the answer key and the settled wait of 60 s (XF-4), or when `.pytest_cache` has not gone within 60 s of the answer key (XF-6) | FAIL |
| XF-5: no clarify card within 90 s | void attempt (the three-attempt rule of XF-5) |
| XF-5: the card does not close within 2 s of Enter, or the reply names the other choice | FAIL |
| XF-2: the reply has ended, or its last counting line is above 1899, at the moment of Esc | void run (too fast); rerun with 6000 numbers as written in XF-2 |
| XF-2: the screen does not settle within 20 s of Esc, or the reply runs on to its end | FAIL |
| XF-1: the highest counting line is 900 or more when typing starts | void run; rerun with 3000 numbers |
| any XF row: provider error text in the pane | void run |
| any row: the gateway cannot be reached or the pane shows a connection failure before the first prompt | `BLOCKED (environment)` |
| any row: reads never show text just typed | `BLOCKED (environment)` |

- **R-8 Stages.** As section 8: BLOCKED ends nothing; a FAIL in stage 1 ends the run; stage 2 is finished before stage 3 starts so that one new candidate fixes everything found; a row whose Given depends on a failed row is run as written and, if its Given cannot be made true, is `BLOCKED (environment)` naming the failed row.
- **R-9 Quitting between rows.** As section 7.2.
- **R-10 AUTO-1.** As the details under AUTO-1 above.
- **R-11 A card open across quit and resume (OP-4).** The requirement R5 in the [requirements](../01-vision-and-requirements.md) says a reopened Talaria shows questions still waiting for an answer, and the roadmap's check says quitting and resuming loses nothing; no other row covers an open card across a quit. P1-12's specification says quit with a card open is allowed and the resume snapshot restores the card (`CanonicalSnapshot.prompts`, fact H30 there). Hermes denies an unanswered approval after `approvals.timeout` (300 s by default), so the operator resumes within 2 minutes. If the card does not come back the row is FAIL with the pane text and the lead files it for the owner of `P1-12`, who finds out whether Talaria or Hermes dropped it. Whether Hermes keeps an approval with no client attached is a **verify at the pinned commit before relying on it** item; if the cause is Hermes, the operator decides on the parent issue whether this clause of OP-4 stands, and the issue is edited before the next candidate.
- **R-12 Names.** The `Runner:` line holds a role. The login-name grep of the issue's AC-11 needs the login name of everyone who ran a row; the lead supplies the list at stage 4 and types it into the command (a person's name is never saved in the repository). A login name shorter than four characters is read by eye.

## 11. Decisions that stay with the operator (per run)

- The length of the day (this document proposes at least four hours; the roadmap says "a day").
- The model and provider for the day, and the repository and branch.
- Which terminals are in the matrix (this document proposes Herdr, tmux and one plain terminal).
- Which stronger model to use for XF-5 when the free model does not call the clarify tool (the issue allows the rerun; the operator names the model).
- Whether the two XF-7 idle limits (growth 4096 KiB, CPU 1 second over two minutes) are right.
- Whether this record, its checker and the checker's tests are acceptable next to draft ADR-0012 (settled decision 8 of the issue).
- Whether Linux is also a platform for this candidate (this document records `OS:` and runs on one).
- Whether the OP-4 clause about a card open across quit and resume stands if Hermes, not Talaria, drops the card (ruling R-11).

## Appendix A: the verbatim scenarios of the accepted issues

Copied from the final bodies of the source issues when this document was written. Placeholders for other issues are written as plain ids. Where a scenario says `Result: ...`, the result goes in the record, not here.

### A.P1-03: scenarios of P1-03

Setup text of the source issue, verbatim:

> Scenarios use the example `reconnect_watch` (unit specification §4.7: it streams one session, prints link states, gaps and resyncs, and reads `drop`, `blackhole`, `refuse`, `restore`, `quit` on standard input). Capture panes with `herdr pane read`, without home paths or user names. "The watch command" is `cargo run -q -p hermes-client --example reconnect_watch -- --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --prompt "<prompt>"`.

#### Row `P1-03 UAT-1`: A dropped connection mid-answer comes back with nothing lost or doubled
- Persona: developer
- Covers: AC-6, AC-9
- Runner: agent tester in Herdr
- Preconditions: operator sets up the gateway at the pinned release serving `fixture` with the `ollama-cloud` key; `cargo build -p hermes-client --examples` done.
- Given the watch command with the prompt "Count from 1 to 300, one number per line."
- When numbers start streaming, the tester types `drop` and presses Enter
- Then `STATE reconnecting attempt=1 retry_in=1.0s` with `reason=socket closed` or `reason=read failed`, then within 5 seconds `STATE connected generation=2`, `REATTACHED`, `REPLAYED <n> events`; numbers continue, none twice; it ends with `TURN COMPLETE status=complete`, `SEQ CHECK delivered=<n> duplicates=0 missing=0` and `RESULT PASS`
- Evidence: the pane text, and the seconds from `drop` to `STATE connected generation=2`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-05: scenarios of P1-05

Setup text of the source issue, verbatim:

> Offline ones need no Hermes. Live ones use the profile `fixture` and the free model `minimax-m3.1-flash` on `ollama-cloud`, never `unattended`. Capture panes with `herdr pane read`, scrubbing home directories, user names and other profile names.

#### Row `P1-05 UAT-3`: End to end on a live gateway: record a session, then reduce it
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: operator sets up the live preconditions above; the tester records fresh logs with P1-04's script (see `tools/record-fixture/`) into `LIVE=$(mktemp -d)`.
- Given fresh frame logs recorded on the current revision
- When the tester runs `TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_FRAMES="$LIVE" cargo test -p talaria-core --test live_reduce -- --ignored --nocapture`
- Then the `LIVE_REDUCE_RESULT: PASSED` lines equal the file count (`ls "$LIVE"/*.jsonl | wc -l`, seven for a full recording), none says `SKIPPED`, the output ends `test result: ok. 1 passed`, and `unknown_event_types:` lists types or `none`
- Evidence: the pane text, the log names, the unknown-type line
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-06: scenarios of P1-06

Setup text of the source issue, verbatim:

> The `talaria` binary arrives in `P1-12`, so these use this unit's demo (section 9; `cargo build --release -p talaria-tui --examples`): `inline_demo --lines N --interval-ms M [--hostile]` commits `line 0001` to `line N` under two live rows containing `LIVE#`; `o` opens an overlay reading `OVERLAY - press Esc`, `Esc` closes it, `q` restores and exits 0. `<pane>` is a Herdr pane (`herdr pane split --current --direction right --cwd .`) running `bash --noprofile --norc` with `PS1='$ '`. Read scrollback with `herdr pane read <pane> --source recent --lines N`, the screen with `--source visible`. First run `echo READ-CHECK` and confirm both sources show it; if not, or `recent` shows no scrollback, the result is BLOCKED (environment). Scrub user, host and home path from evidence.

#### Row `P1-06 UAT-1`: Committed lines reach Herdr's scrollback exactly once, from the top row
- Persona: developer
- Covers: AC-1, AC-5
- Runner: agent tester in Herdr
- Preconditions: Herdr running; the demo built.
- Given an empty `<pane>`
- When the tester runs `herdr pane run <pane> 'clear; target/release/examples/inline_demo --lines 300 --interval-ms 5'`, waits for `line 0300`, sends `herdr pane send-keys <pane> q`, then reads `--source recent --lines 400`
- Then `line 0001` to `line 0300` each appear exactly once, in order, and no line contains `LIVE#`
- Evidence: the read text and a count per tag
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-06 UAT-2`: A resize in tmux neither duplicates nor loses lines
- Persona: developer
- Covers: AC-1, AC-5
- Runner: agent tester in Herdr
- Preconditions: tmux installed; the demo built; at the repository root.
- Given `tmux new-session -d -s p106 -x 100 -y 30 'bash --noprofile --norc'` and `tmux send-keys -t p106 'target/release/examples/inline_demo --lines 200 --interval-ms 20' Enter`
- When, after `line 0100` appears, the tester runs `tmux resize-window -t p106 -x 80 -y 20`, then `tmux resize-window -t p106 -x 100 -y 40`, waits for `line 0200`, runs `tmux send-keys -t p106 q`, `tmux capture-pane -p -S - -t p106` and `tmux kill-session -t p106`
- Then `line 0001` to `line 0200` each appear exactly once, in order, and no line contains `LIVE#`
- Evidence: the `capture-pane` text and per-tag counts
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-06 UAT-3`: Closing an overlay restores the inline view and leaves nothing behind
- Persona: developer
- Covers: AC-1, AC-5
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given `herdr pane run <pane> 'target/release/examples/inline_demo --lines 50'` has committed `line 0050`
- When the tester reads `--source visible`, sends `o`, reads `visible`, sends `esc`, reads `visible`, sends `q`, then reads `--source recent --lines 200`
- Then the second read shows `OVERLAY - press Esc` and no tag; the third equals the first; the last has no `OVERLAY` line and each tag once
- Evidence: the four reads
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-06 UAT-4`: Terminal matrix: resize, overlay, selection, copy and search by hand
- Persona: developer
- Covers: AC-6
- Runner: operator
- Preconditions: operator sets up Herdr, tmux and one plain terminal (Ghostty, iTerm2 or Terminal.app); the demo built. Run once in each.
- Given `target/release/examples/inline_demo --lines 300 --interval-ms 20` running there
- When, before `line 0300`, the operator shrinks the pane or window by about ten rows and enlarges it again, presses `o`, then `Esc`; after `line 0300`, presses `q`; selects `line 0150` with that terminal's own selection (a multiplexer's copy mode counts), copies and pastes it; and searches (its own search, or the host terminal's; record which) for `line 0150`, `line 0007` and `LIVE#`
- Then the overlay showed `OVERLAY - press Esc` and the live rows returned after `Esc`; the paste is exactly `line 0150`; `line 0150` and `line 0007` are each found once; `LIVE#` is not found
- Evidence: screenshots linked from the matrix row; the pull request body records `UAT-4 (<terminal>): PASS` or `FAIL`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-06 UAT-5`: Without a terminal, the demo refuses cleanly (negative)
- Persona: scripter
- Covers: AC-1, AC-5
- Runner: agent tester in Herdr
- Preconditions: the demo built.
- Given a shell at the repository root
- When the tester runs `bash -c 'target/release/examples/inline_demo --lines 5 </dev/null 2>&1 | cat -v; echo "exit=${PIPESTATUS[0]}"'`
- Then the output is exactly two lines, `inline_demo: standard input or output is not a terminal` and `exit=2`, with no `^[` and no `panicked`
- Evidence: the output
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-06 UAT-6`: Hostile text cannot clear or move the screen (negative)
- Persona: developer
- Covers: AC-1, AC-5
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given `<pane>` after `herdr pane run <pane> 'echo BEFORE-HOSTILE'`
- When the tester runs `herdr pane run <pane> 'target/release/examples/inline_demo --lines 20 --hostile'`, waits for `line 0020`, sends `q`, and reads `--source recent --lines 100`
- Then `BEFORE-HOSTILE` appears once; one row reads exactly `HOSTILE ␛[2J␛[H␇‹U+202E› END`; `line 0001` to `line 0020` appear once, in order
- Evidence: the read text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-07: scenarios of P1-07

Setup text of the source issue, verbatim:

> Scenarios use the demo program from the repository root. Build first: `cargo build -q -p talaria-tui --example markdown_demo`. `$T` is a fresh `mktemp -d` directory, `$D` stands for `cargo run -q -p talaria-tui --example markdown_demo --`, and `$F` is `crates/talaria-tui/tests/fixtures/markdown`.

#### Row `P1-07 UAT-1`: Every construct renders as text
- Persona: developer
- Covers: AC-5
- Runner: agent tester in Herdr
- Preconditions: a Herdr pane in the repository; the example is built.
- Given `$F/sample.md` (single-width characters only; no code line starts with `#`)
- When the tester runs `$D $F/sample.md --width 80 > $T/sample.txt`, then `grep -c -E '^#{1,6} ' $T/sample.txt`, `python3 -c "import sys; print(sum(len(l.rstrip('\n'))>80 for l in open(sys.argv[1], encoding='utf-8')))" $T/sample.txt`, `grep -c -E '^(•|☐|▎ |┌|─)' $T/sample.txt` and `grep -c '^rust$' $T/sample.txt`
- Then the four counts are 0, 0, at least 5, and at least 1 (no heading marks, no line over 80 characters; list, task, quote, table, rule and code label lines present)
- Evidence: the four counts and the first 40 lines of `$T/sample.txt`
- Result: NOT RUN

#### Row `P1-07 UAT-2`: Streaming equals whole rendering and commits early
- Persona: developer
- Covers: AC-4
- Runner: agent tester in Herdr
- Preconditions: as UAT-1, with `$T/sample.txt` from it.
- Given the same fixture
- When the tester runs `$D $F/sample.md --width 80 --stream --no-live --delta-bytes 7 --trace > $T/stream.txt 2> $T/trace.txt`, then `diff $T/sample.txt $T/stream.txt; echo $?`, `grep -n -E 'committed=[1-9]' $T/trace.txt | head -1` and `wc -l < $T/trace.txt`
- Then the diff prints nothing and the exit code line is `0`; the grep line number is smaller than the `wc -l` count (a block committed before the last push)
- Evidence: the exit code, the grep line and the count
- Result: NOT RUN

#### Row `P1-07 UAT-3`: Hostile text cannot move the cursor or clear the screen (negative case)
- Persona: developer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: a Herdr pane `<pane>`; the example is built.
- Given `$F/hostile.md` (at most 15 lines): a clear-screen sequence, a window-title sequence, a colour sequence and a right-to-left override
- When the tester runs `printf 'BEFORE\n'; $D $F/hostile.md --width 80` in `<pane>`, then `herdr pane read <pane> --source visible`
- Then the pane text still shows `BEFORE` above the document, and shows the literal text `␛[2J` and `‹U+202E›`
- Evidence: the pane text
- Result: NOT RUN

#### Row `P1-07 UAT-4`: Narrow widths and bad input fail plainly (negative case)
- Persona: developer
- Covers: AC-5
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given the sample fixture, whose table fits at 80 columns and not at 24
- When the tester runs `$D $F/sample.md --width 24 | grep -c -E '^(Name|Qty): '`, then `$D $F/sample.md --width 3 2>&1 >/dev/null | head -1`, then `$D /no/such/file; echo $?`
- Then the first prints a number above 0 (the table fell back to rows), the second prints exactly `note: width 3 raised to 8`, and the third prints `cannot read /no/such/file:` with a reason on standard error, then `2`
- Evidence: the three outputs
- Result: NOT RUN

#### Row `P1-07 UAT-5`: The frame budget holds
- Persona: builder
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: no other heavy process running.
- Given the four benchmark documents
- When `cargo bench -p talaria-tui --bench markdown_stream; echo $?`
- Then four scenarios are listed, each with a median-of-three max frame time at or below 16 ms and status `pass`, and the exit code is `0`
- Evidence: the table, the machine description, the commit
- Result: NOT RUN

#### Row `P1-07 UAT-6`: It looks right in real terminals
- Persona: developer
- Covers: AC-5, AC-10
- Runner: operator
- Preconditions: operator sets up a Herdr pane, a tmux pane and a plain terminal window.
- Given the sample fixture and the 64 KB document
- When in each terminal the operator runs `$D $F/sample.md`, then `$D --generate mixed-64k --stream --delta-bytes 64 --delay-ms 5`, then `NO_COLOR=1 $D $F/sample.md`
- Then headings are bold, links blue and underlined, code shows string, comment and number colours, tables line up, the streamed run leaves no leftover tail lines or flicker, and with `NO_COLOR=1` no colour appears but bold and underline remain; the operator marks each terminal
- Evidence: a screenshot of each terminal after each command
- Result: NOT RUN


### A.P1-08: scenarios of P1-08

Setup text of the source issue, verbatim:

> The scenarios use this unit's demo (the binary comes in `P1-12`), built with `cargo build --release -p talaria-tui --examples`, run from the repository root. In Herdr, `<pane>` comes from `herdr pane split --current --direction right --cwd .`; read it with `herdr pane read <pane> --source recent --lines 80`. Run `sleep 0.3` after every `send-text` (else it looks like a burst). No gateway.

#### Row `P1-08 UAT-1`: Type, send, recall history, and press keys that must do nothing
- Persona: developer
- Covers: AC-3, AC-8
- Runner: agent tester in Herdr
- Preconditions: Herdr running; the demo built.
- Given `herdr pane run <pane> 'clear; target/release/examples/composer_demo'` shows the empty box
- When the tester runs `herdr pane send-keys <pane> ctrl+b ctrl+s f1 f2`, reads, sends `first line`, `ctrl+j`, `second line`, reads, `enter`, reads; then sends `alpha` Enter, `beta` Enter, `gam`, `up`, `up`, `down`, `down`, reading after each
- Then every command exits 0; the first read shows an unchanged empty box; the second both lines, no `SUBMITTED`; the third `SUBMITTED lines=2 chars=22 text="first line\nsecond line"` above an empty box; the history reads show `beta`, `alpha`, `beta`, `gam`, with exactly three `SUBMITTED` lines
- Evidence: the reads and exit codes; if Herdr rejects a key name, send that key with `tmux send-keys` and record it
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-08 UAT-2`: A 10,000 character paste arrives as one event
- Persona: developer
- Covers: AC-5, AC-8, AC-11
- Runner: agent tester in Herdr
- Preconditions: `tmux`, `python3`; the demo built.
- Given `TMP=$(mktemp -d)`, `python3 -c "print(('x'*99+'\n')*99+'x'*100,end='')" > "$TMP/paste.txt"` and `tmux new-session -d -s p108 -x 80 -y 24 'target/release/examples/composer_demo'`
- When the tester runs `tmux load-buffer "$TMP/paste.txt"`, `tmux paste-buffer -p -t p108`, waits 1 second, `tmux capture-pane -p -S - -t p108`, `tmux send-keys -t p108 Enter`, captures again, `tmux kill-session -t p108`
- Then the first capture has exactly one `PASTE-EVENT chars=10000`, no `SUBMITTED`, and a box of at most 10 rows; the second has exactly one `SUBMITTED lines=100 chars=10000`
- Evidence: both captures
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-08 UAT-3`: Esc and Ctrl+C
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given the demo started with `--running`
- When the tester types `keep me`, sends `esc`, `ctrl+c`, `ctrl+c`, reading after each; restarts the demo without `--running`; sends `ctrl+c`, reads, waits 3 seconds, reads, sends `ctrl+c` twice, reads
- Then `esc` gives one `INTERRUPT` and keeps `keep me`; the first `ctrl+c` empties the box, still one `INTERRUPT`; the second makes two. Without `--running` the first `ctrl+c` shows `Press Ctrl+C again to exit` on the bottom border, gone after 3 seconds (a tick with no key); of the last two presses the first re-arms it, the second prints `EXIT-REQUESTED` and returns to the shell prompt
- Evidence: the reads
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-08 UAT-4`: A mistake in the config file is explained, not fatal (negative)
- Persona: developer
- Covers: AC-4, AC-8
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given `TMP=$(mktemp -d)`, `printf 'keys = [\n' > "$TMP/bad.toml"` and `printf '[keys]\nsubmit = ["banana"]\nbogus = "x"\n' > "$TMP/odd.toml"`
- When the tester runs the demo with `--config "$TMP/bad.toml"`, types `hi` Enter, reads, quits, then repeats with `--config "$TMP/odd.toml"`
- Then the first run shows exactly one `NOTICE: Talaria could not read ~/.talaria/config.toml (line ...` line ending `Using the default settings.`, no `$TMP` text, and `hi` still sends (`SUBMITTED lines=1 chars=2 text="hi"`); the second shows exactly three `NOTICE:` lines in order (unknown action `bogus`, bad chord `banana`, no usable key: Enter stays) and Enter still sends
- Evidence: the reads
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-08 UAT-5`: Shift+Enter and word keys in the operator's real terminals
- Persona: developer
- Covers: AC-9
- Runner: operator
- Preconditions: the operator names each terminal in daily use (for example Ghostty, iTerm2, Terminal.app, also in Herdr and tmux).
- Given the demo running there
- When the operator types `a`, Shift+Enter, `b`, Alt+Enter, `c`, Ctrl+J, `d`, Enter; then `one two`, Option plus Left, `X`, Enter
- Then `SUBMITTED lines=4 chars=7 text="a\nb\nc\nd"` appears, then `text="one Xtwo"`; where Shift+Enter sent early, record "Shift+Enter unsupported" (Alt+Enter and Ctrl+J must work everywhere); record where Option plus Left failed
- Evidence: a "Terminal matrix" table in the pull request, one row per terminal and multiplexer, one pasted line each
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-09: scenarios of P1-09

Setup text of the source issue, verbatim:

> The binary arrives in `P1-12`, so the scenarios run this unit's demo (unit specification section 13: scenario scripts, log line formats, effect loop), built with `cargo build --release -p talaria-app --examples`. `<pane>` is opened at the repository root with `herdr pane split --current --direction right --cwd .`; a scenario starts with `herdr pane run <pane> 'target/release/examples/prompt_cards --scenario <name> --log target/uat/<name>.log'`. Screen reads: `herdr pane read <pane> --source visible`. "The log" is `target/uat/<name>.log`. Before each key, wait for the card: `until grep -q 'card shown' target/uat/<name>.log; do sleep 0.2; done; sleep 0.5` (the 250 ms guard).

#### Row `P1-09 UAT-1`: Approve a dangerous command once
- Persona: developer
- Covers: AC-2, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built; `fixtures/frames/approval.jsonl` present.
- Given the card for `rm -rf .pytest_cache` from `--scenario approval`
- When the tester reads the screen, sends `1`, waits for `scenario done`, reads again
- Then the first read has the command and lines containing `1. once`, `2. session`, `3. always`, `4. deny` (fewer offered: BLOCKED (environment), fixture reported); the second no card; the log one `out` `approval.respond` with exactly `session_id`, `execution_generation`, `prompt_id`, `"choice":"once"`
- Evidence: both reads and the log
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-09 UAT-2`: Approve for the session with a note
- Persona: developer
- Covers: AC-4, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built.
- Given the approval card from `--scenario approval-note`
- When the tester sends `tab`, runs `herdr pane send-text <pane> 'use a dry run first'`, sends `tab` then `2`, and waits for `scenario done`
- Then the log shows, in order, `out` `approval.respond` with `"choice":"session"`, its `in` reply `"status":"resolved"`, and one `out` `prompt.submit` with exactly `session_id`, `text` (`use a dry run first`), `submission_id`, no `unattended`
- Evidence: the log
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-09 UAT-3`: Answer a multi-select question, then skip one
- Persona: developer
- Covers: AC-1, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built.
- Given `--scenario clarify-multi`: "Which environments?" (`staging`, `prod`, `qa`, multi-select), then "Any notes?" (no choices)
- When the tester sends `space`, `down`, `space`, `enter`; waits for a second `card shown: Question` line, then 0.5 s; sends `esc`; waits for `scenario done`
- Then the first `out` `clarify.respond` has `"answer":"[\"staging\",\"prod\"]"`, the second `"answer":""`, each with the three key fields
- Evidence: the log and a screen read
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-09 UAT-4`: A sudo password never appears on screen or in the log
- Persona: developer
- Covers: AC-5, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built.
- Given the card `sudo password` with `$ sudo apt-get install jq` from `--scenario sudo`
- When the tester runs `herdr pane send-text <pane> 'TALARIA-MASK-7f3a'`, reads, sends `enter`, waits for `scenario done`, reads again
- Then the first read shows `value: ` and 17 bullets; the second no card; the log has an `out` value `[withheld, 17 characters]`; no read and not the log holds `TALARIA-MASK-7f3a`
- Evidence: both reads, the log, `grep -c TALARIA-MASK` of each (expected 0)
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-09 UAT-5`: Unsupported requests are refused at once with one notice (negative)
- Persona: developer
- Covers: AC-3, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built.
- Given `--scenario unsupported`: the mock sends `theme.pick` twice, then `terminal.read`
- When the tester waits for `scenario done` and reads the screen
- Then the log has three `out` -32601 errors naming their kinds; three `refused <kind> in <N> ms` lines, each N below 100; exactly two `notice:` lines (`theme.pick`, `terminal.read`); no `card shown`
- Evidence: the log and the read
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-09 UAT-6`: Ctrl+C with an approval open denies first, then interrupts (negative)
- Persona: developer
- Covers: AC-4, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built; `fixtures/frames/approval.jsonl` present.
- Given the approval card from `--scenario interrupt`
- When the tester sends `ctrl+c` and waits for `scenario done`
- Then the log shows `out` `approval.respond` `"choice":"deny"` and its `in` reply before `out` `session.interrupt`; `card closed`; no `answered elsewhere` notice
- Evidence: the log and a read
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-09 UAT-7`: Prompts settled or withdrawn elsewhere disappear (negative)
- Persona: developer
- Covers: AC-3, AC-9
- Runner: agent tester in Herdr
- Preconditions: the demo built.
- Given `--scenario withdrawn`: an approval settled after 2 s, then a `sudo` card withdrawn (`request.cancel`, `reason: "timeout"`) after 2 s
- When the tester waits for `scenario done` without pressing a key
- Then the log has `card shown` then `card closed` for each card, the notices `answered elsewhere or timed out` and `timed out; the agent continued without it`, and no `out` answer to either
- Evidence: the log and a final read
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-10: scenarios of P1-10

Setup text of the source issue, verbatim:

> Offline scenarios: no Hermes, no credentials. Agent testers run them in Herdr and capture the pane with `herdr pane read`. Build first: `cargo build -q -p talaria-tui --example status_demo`. Make helper scripts executable in a `mktemp -d` directory; strip any home directory or user name from evidence.

#### Row `P1-10 UAT-1`: The line shortens by priority as the terminal narrows
- Persona: developer
- Covers: AC-4, AC-5
- Runner: agent tester in Herdr
- Preconditions: the example is built.
- Given the `full` fixture (specification section 4.4: external text `tests: 296 passing`)
- When the tester runs `cargo run -q -p talaria-tui --example status_demo -- render --fixture full --cols 160`, then the same with `--cols 100`, `60`, `40` and `10`
- Then widths 160 and 100 print the same single line ` opus-5.5 high │ ~/proj (main) │ ctx 34% │ $0.12 │ ⚑ 2 need you │ 3 working │ tests: 296 passing`; 60 prints ` opus-5.5 high │ ~/proj (main) │ ctx 34% │ ⚑ 2 need you`; 40 prints ` opus-5.5 high │ ctx 34% │ ⚑ 2 need you`; 10 prints ` ⚑ 2 need…`; each output is one line no wider than its `--cols`
- Evidence: the five lines with their commands
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-10 UAT-2`: A status command receives the documented JSON
- Persona: configurer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: script `echo-stdin.sh` (`#!/bin/sh`, `printf 'got: '`, `cat`).
- Given the `full` fixture
- When the tester runs `status_demo document --fixture full`, then `status_demo run-command -- ./echo-stdin.sh`
- Then the first prints exactly one line, `{"version":1,"mode":"live","connection":"connected","session":{"id":"sess-1","title":null},"turn":"idle","pending_prompts":0,"subagents":{"active":0,"terminal":0},"usage":null}` (the eight keys in this order); the second prints `outcome=ok` and a `row[0]=` equal to `got: ` followed by that same line
- Evidence: both outputs (the exact first line fixes the key order)
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-10 UAT-3`: The command's environment is default-deny
- Persona: configurer
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: script `env.sh` (`#!/bin/sh`, `env | sort | tr '\n' ' '; echo`).
- Given the demo is started with `OPENAI_API_KEY=canary-key`, `MY_CANARY=canary-value`, `LANGCHAIN_API_KEY=canary-lang`, `TALARIA_GATEWAY_URL=ws://gateway.invalid:9/ws?ticket=canary-ticket` and `TALARIA_PROFILE=fixture` set
- When the tester runs `status_demo run-command -- ./env.sh`
- Then the `row[0]=` line contains `PATH=`, `HOME=`, `TALARIA_PROFILE=fixture` and `TALARIA_GATEWAY_URL=ws://gateway.invalid:9/ws ` (ends at the path, no `?`), and `canary` appears nowhere in the output
- Evidence: the output, and `grep -c canary` printing 0
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-10 UAT-4`: Failures and hostile output give plain markers, never crash, and leave no process behind
- Persona: developer
- Covers: AC-8, AC-10
- Runner: agent tester in Herdr
- Preconditions: scripts `slow.sh` (`#!/bin/sh`, `sleep 60`), `fail.sh` (`#!/bin/sh`, `echo oops >&2`, `exit 3`), `quiet.sh` (`#!/bin/sh`, `true`), `hostile.sh` (`#!/bin/sh`, `printf '\033[2Jred \033[31mtext\033[0m \342\200\256evil\n'`, `seq 1 12`).
- Given a shell where `pgrep -f 'sleep 60'` prints nothing
- When the tester runs `status_demo run-command -- ./slow.sh`, then `-- ./fail.sh`, `-- ./quiet.sh`, `-- ./missing-program`, then `pgrep -f 'sleep 60'`, then `status_demo run-command --render-cols 160 -- ./hostile.sh > out.txt; grep -c "$(printf '\033')" out.txt; cat out.txt`
- Then the first four outputs show, in order: `outcome=timed_out`, `marker=status: command timed out`, `elapsed_ms` 1900 to 2600; `marker=status: command failed (exit 3): oops`; `marker=status: no output`; `marker=status: command not found`; `pgrep` prints nothing; the demo exits 0 each time; for `hostile.sh` the `grep -c` prints 0 (no raw escape byte was printed) and `out.txt` has `rows=8`, `truncated=true`, `row[7]=… truncated (13 rows)` and a `line=` text containing `␛` (U+241B), the stand-in `P1-06`'s `sanitize` gives ESC
- Evidence: the outputs, the empty `pgrep`, the `0` and `out.txt`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-10 UAT-5`: The command reruns only when its input changes or its interval passes
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: script `count.sh` with `#!/bin/sh`, `echo x >> "$COUNT_FILE"` and `echo ran`; an empty file `$f`.
- Given an empty count file; the demo changes `pending_prompts` by adding 1 at each change
- When the tester runs `COUNT_FILE=$f status_demo watch --seconds 10 --interval 3 --change-every 0 --allow COUNT_FILE -- ./count.sh`, empties the file, then runs it again with `--change-every 2`
- Then the first prints `spawns=4` (offsets 0, 3, 6, 9 seconds, each within 0.3 s); the second prints `spawns=5` (offsets 0, 2, 4, 6, 8, each within 0.3 s; the demo ticks every 100 ms from 0 to 9.9 s, none at 10 s); `wc -l` of the file equals `spawns` each time
- Evidence: both outputs and `wc -l` results
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-11: scenarios of P1-11

Setup text of the source issue, verbatim:

> Build once: `cargo build --release -p talaria-cli -p talaria-bench`. `<pane>` comes from `herdr pane split --current --direction right --cwd .`. Replay files: `target/release/talaria-bench generate --out "$TMPDIR/bench-files"`.

#### Row `P1-11 UAT-1`: The table on the real binary
- Persona: builder
- Covers: AC-6, AC-12
- Runner: agent tester in Herdr
- Preconditions: release build done.
- Given a clean pane
- When the tester runs `herdr pane run <pane> 'target/release/talaria-bench run --binary target/release/talaria --report "$TMPDIR/bench.json"; echo EXIT=$?'`, waits for `EXIT=`, then `herdr pane read <pane> --source recent --lines 80`
- Then, in the table (the lines from `os: ` to just before `EXIT=`; the shell prompt and typed command are outside it), the first line starts `os: `, every REQ-14 row ends PASS, FAIL, INFO or SKIPPED (the live gateway row SKIPPED) and no absolute path appears; the last pane line is `EXIT=0` if no row is FAIL, else `EXIT=1`
- Evidence: pane text, `bench.json`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-11 UAT-2`: A slow client turns the run red, and a missing binary is not a pass (negative)
- Persona: builder
- Covers: AC-5, AC-6
- Runner: agent tester in Herdr
- Preconditions: release build done.
- Given a clean pane
- When the tester runs `herdr pane run <pane> 'target/release/talaria-bench run --binary target/release/fake-tui --binary-arg --startup-delay-ms --binary-arg 300 --only launch; echo EXIT=$?; target/release/talaria-bench run --binary ./no-such-binary; echo EXIT=$?'`, then reads the pane
- Then the launch row shows at least 300 ms and FAIL, then `EXIT=1`; next one plain line says `./no-such-binary` was not found and to build it with `cargo build --release -p talaria-cli`, no table, then `EXIT=2`
- Evidence: the pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-11 UAT-3`: Watch a recorded reply stream and type over it
- Persona: developer
- Covers: AC-2, AC-11
- Runner: agent tester in Herdr
- Preconditions: release build; replay files generated.
- Given a clean pane
- When the tester runs `herdr pane run <pane> 'target/release/talaria --bench-replay "$TMPDIR/bench-files/reference-stream.jsonl"'`, waits 10 s, sends `herdr pane send-keys <pane> h e l l o`, reads the pane, waits 60 s, then reads with `--source recent --lines 200`
- Then the first read shows streamed Markdown (the text `Reference turn 1` as a heading, with or without its `##` marks, depending on the renderer) above a composer containing `hello`; the second contains the line `End of reference turn 2.` once, the composer, and no `kbdprobe`
- Evidence: the two pane reads
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-11 UAT-4`: Bad input leaves the terminal usable (negative)
- Persona: developer
- Covers: AC-2, AC-11
- Runner: agent tester in Herdr
- Preconditions: release build done.
- Given a clean pane
- When the tester runs `herdr pane run <pane> 'target/release/talaria --bench-replay no-such-file.jsonl; echo EXIT=$?; target/release/talaria --bench-replay x.jsonl --bench-speed 0; echo EXIT=$?'`, then `herdr pane send-keys <pane> a b c`, and reads the pane
- Then each command prints one plain line (the first names `no-such-file.jsonl`, the second says "--bench-speed must be instant, recorded or a positive number") then `EXIT=2`, and `abc` is echoed at the prompt
- Evidence: the pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-11 UAT-5`: The macOS run on demand
- Persona: builder
- Covers: AC-8
- Runner: operator
- Preconditions: the workflow file is pushed on the branch (or on `main`).
- Given the workflow file in place
- When the operator runs `gh workflow run bench.yml --ref p1-11-performance-harness -f runner=macos-14` (omit `--ref` once merged), then `gh run watch`
- Then the run finishes and its log prints the table, whose first line is `os: macos arch: <runner's architecture> cpus: <n>`
- Evidence: the run URL and the table
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-12: scenarios of P1-12

Setup text of the source issue, verbatim:

> `<pane>` and `<pane2>` come from `herdr pane split --current --direction right --cwd .`; `<talaria>` is `talaria` after `cargo build --release -p talaria-cli` and `export PATH="$PWD/target/release:$PATH"` once in each pane from the repository root (`command -v talaria` must name that build; an old global install exists); `<flags>` is `--profile fixture --provider ollama-cloud --model minimax-m3.1-flash`; `<project>` is a scratch folder from `mktemp -d`; the tester types with `herdr pane run <pane> '<text>'` and first types a marker to find which read source (`--source visible` or `recent-unwrapped`) shows it; a read lacking the text just typed is stale and never evidence. Redact paths before pasting (spec 11). Each result goes in the pull request as `UAT-<n>: PASS` (or FAIL, BLOCKED, NOT RUN) plus evidence; only PASS counts.

#### Row `P1-12 UAT-1`: Ask a question in a new session, then quit
- Persona: developer
- Covers: AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: operator sets up the fixture profile and gateway.
- Given `<pane>` is at `<project>`
- When the tester runs `<talaria> <flags>`, waits for the composer, submits `What is 1234 plus 4321? Reply with only the number.`, waits for `5555`, reads the pane, runs `/quit`, then reads `--source recent --lines 200`
- Then the first read shows the question, `5555` once and `minimax-m3.1-flash` on the bottom line; the second shows the shell prompt with no composer box and, directly above it, `Resume this session with:` and `  talaria --resume <id> --profile fixture`
- Evidence: both reads, the session id, time to the composer
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-2`: Quitting while the agent works leaves the turn running
- Persona: developer
- Covers: AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given a new session at `<project>`
- When the tester submits `Count from 1 to 400, one number per line, then write the sum of 6000 and 1731 on the last line.`, runs `/quit` while the status line shows working (if the turn already finished, repeat with 1000 numbers), waits 60 seconds, runs `<talaria> --resume <id> --profile fixture` and reads the pane
- Then the quit printed `The agent is still working on it in the background.` above the resume lines, and the resumed transcript shows the prompt and a reply ending in `7731`, no line twice
- Evidence: both pane reads and the id
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-3`: Continue the most recent session of this folder
- Persona: developer
- Covers: AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: UAT-2's session.
- Given `<pane>` is at `<project>`
- When the tester runs `<talaria> -c --profile fixture`, submits `Reply with only the word OK.`, waits for the answer, runs `/quit` and reads the pane
- Then UAT-2's prompt is visible under a line starting `── Resumed:`, `OK` is answered, and the printed id equals UAT-2's
- Evidence: pane text and both ids
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-4`: Continue in a folder with no earlier session is refused (negative)
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: other folders hold sessions (UAT-3).
- Given `<pane>` is in a new empty folder
- When the tester runs `<talaria> -c --profile fixture; echo "exit=$?"`
- Then the output ends with `talaria: no earlier session found for this folder.`, a `Next step:` line and `exit=1`
- Evidence: pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-5`: No gateway and an unknown profile give plain errors (negative)
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: gateway running for the second command only.
- Given a shell
- When the tester runs `HERMES_HOME=$(mktemp -d) <talaria> --no-ensure; echo "exit=$?"`, then `<talaria> --profile nosuchprofile; echo "exit=$?"`
- Then the first prints `talaria: no Hermes gateway is running for this home`, a `Next step: Run` line naming `hermes gateway ensure --json` and `exit=1`; the second prints `talaria: the gateway never discovered a profile named nosuchprofile`, a `Next step:` line and `exit=1`; no path or escape garbage
- Evidence: both outputs
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-6`: Answer an approval for `rm -rf .pytest_cache`
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: operator sets `approvals.mode: manual` on the fixture profile; `<project>` holds `.pytest_cache/x`. Without it: BLOCKED (environment), unit not done.
- Given a new session at `<project>`
- When the tester submits `Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache`, waits for the card, presses Esc (deny), runs `ls -a` in `<pane2>` (at `<project>`), submits it again, presses Enter (the highlight starts on `once`), then lists again
- Then the card shows the command and numbered choices; the first listing still shows `.pytest_cache`; after the approval the second does not, and the agent says so
- Evidence: both cards and both listings
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-7`: A thirty-minute working session
- Persona: developer
- Covers: AC-10
- Runner: either
- Preconditions: as UAT-6.
- Given a new session in a scratch project
- When the runner works 30 minutes: a question, a file edit (`Create hello.txt containing the word hello`), the UAT-6 approval, an interrupt (Esc), a quit and a resume by id, reading the pane at minutes 0, 10, 20 and 30
- Then every step behaves as in UAT-1 to UAT-6, `hello.txt` exists, the scrollback read shows each answer once, and nothing is stale or garbled
- Evidence: the four reads, the scrollback, `ps -o rss= -p <talaria pid>` at minute 30
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-12 UAT-8`: Pick a working session from the list
- Persona: developer
- Covers: AC-10
- Runner: agent tester in Herdr
- Preconditions: as UAT-2.
- Given a session whose long turn (UAT-2's prompt, 1000 numbers) was quit while working
- When the tester at once runs `<talaria> --resume --profile fixture`, types `1`, Enter, reads the pane, quits, runs it again, presses Enter at the question and runs `echo "exit=$?"`
- Then the list shows that session with `working` and its id; picking it shows its transcript under `── Resumed:`; the second run prints `Cancelled.` and `exit=0`
- Evidence: both pane reads and the id
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN


### A.P1-13: scenarios of P1-13

Setup text of the source issue, verbatim:

>

#### Row `P1-13 UAT-1`: Full smoke run on the candidate
- Persona: builder
- Covers: AC-4, AC-5, AC-12
- Runner: agent tester in Herdr
- Preconditions: operator sets up the Dependencies section's live preconditions; `cargo build -p talaria-cli` done on the candidate.
- Given a Herdr pane in the repository root, `herdr tab list` noted
- When the tester runs `TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria target/debug/talaria; echo "exit=$?"`
- Then it prints `tab: <id>`, `PASS` for each of the six steps, `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 300 and `exit=0`; afterwards no `talaria-smoke-` tab is listed
- Evidence: pane text; `herdr tab list` before and after
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-13 UAT-2`: A failing step prints the pane text and closes the tab (negative)
- Persona: builder
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: none. A stub that exits at once: `STUB="$(mktemp -d)/talaria"; printf '#!/bin/sh\necho "stub: no gateway"\nexit 7\n' > "$STUB"; chmod +x "$STUB"`.
- Given a Herdr pane in the repository root, `herdr tab list` noted
- When the tester runs `cargo run -q -p smoke -- run --talaria "$STUB"; echo "exit=$?"`
- Then output shows `FAIL startup (talaria exited with code 7 before showing its screen)`, a pane text block with `stub: no gateway`, `SMOKE_RESULT: FAILED (startup)` and `exit=1`; no `talaria-smoke-` tab remains; no home, login or temporary path is printed
- Evidence: pane text; `herdr tab list` afterwards
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-13 UAT-3`: Outside Herdr nothing is created (negative)
- Persona: builder
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: none.
- Given `herdr tab list` noted
- When the tester runs `env -u HERDR_ENV cargo run -q -p smoke -- run; echo "exit=$?"`
- Then one plain sentence says the test must run inside a Herdr pane, no `SMOKE_RESULT` line appears, `exit=2` follows, and `herdr tab list` is unchanged
- Evidence: pane text; both tab lists
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-13 UAT-4`: Ctrl+C or SIGTERM during a run still closes the tab (negative)
- Persona: builder
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: `cargo build -p smoke` done (pane `A` runs `target/debug/smoke` directly, so the foreground list shows one `smoke` process); a stub that reads input forever: `STUB="$(mktemp -d)/talaria"; printf '#!/bin/sh\necho ready\nwhile read line; do :; done\n' > "$STUB"; chmod +x "$STUB"`.
- Given a second pane `A` from `herdr pane split "$HERDR_PANE_ID" --direction down --no-focus --cwd "$PWD"` (its `.result.pane.pane_id`)
- When the tester runs `herdr pane run A "target/debug/smoke run --talaria '$STUB'"` (double quotes outside, so `$STUB` expands), waits with `herdr pane wait-output A --match 'tab: ' --timeout 60000`, then `herdr pane send-keys A ctrl+c`; then repeats, sending `kill -TERM` to the foreground process whose `cmdline` starts with the `smoke` path (from `herdr pane process-info --pane A`) instead
- Then in both runs pane `A` shows `SMOKE_RESULT: FAILED (interrupted)` within 10 s and no `talaria-smoke-` tab is listed
- Evidence: `herdr pane read A`; `herdr tab list` afterwards
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### Row `P1-13 UAT-5`: The approval step skips when its precondition is absent
- Persona: builder
- Covers: AC-9
- Runner: operator
- Preconditions: operator sets profile `fixture` to `approvals.mode: off` for this scenario only (restored afterwards; gateway restart if needed).
- Given the setup above and a built candidate
- When the operator runs `cargo run -q -p smoke -- run; echo "exit=$?"`, then `TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run; echo "exit=$?"`
- Then the first run prints `SKIP approval (` plus REQ-6's reason and `)`, `SMOKE_RESULT: PASSED WITH SKIPS (approval)` and `exit=3`; the second prints `FAIL approval (skipped: ...; TALARIA_LIVE_REQUIRE=1)`, `SMOKE_RESULT: FAILED (approval)` and `exit=1`
- Evidence: pane text of both
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN
