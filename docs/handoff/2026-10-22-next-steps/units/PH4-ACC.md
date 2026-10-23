# PH4-ACC: Phase 4 acceptance: configure a provider, an MCP server, a profile's model and a cron job without a YAML file, and prove phase 4b as a whole

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order, and how the result is recorded. This document holds what the people doing the work would otherwise have to guess: how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (copied from the phase 4a acceptance unit's and run while drafting), the exact steps and pass conditions of the 11 cross-feature rows and of the operator's own check (the roadmap's phase 4 check), the operator sitting, the run order and the rulings, and the design of the `phase4b` smoke suite (steps, keys, exact reason texts, time budget, tests). Sections are numbered so the issue can point at them.

Handoff documents are linked relatively: [the roadmap's phase 4](../07-roadmap-and-work-units.md#phase-4--configure-hermes-from-talaria), [the P1-13 roadmap entry](../07-roadmap-and-work-units.md#p1-13--live-smoke-test-in-herdr), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here). This unit is the second half of the phase 4 gate. [PH4A-ACC](PH4A-ACC.md) proved the settings screen, providers, model roles, profiles and the doctor; this unit copies its record, checker, evidence rules and smoke extension, changes what must change, and says so where it does. Where this document says "as PH4A-ACC", read that document's section of the same name.

## 1. What this unit is for, in plain terms

Six issues of the phase 4b capability each passed their own scenarios on their own commit: MCP servers (P4-04), skills, toolsets and plugins (P4-05), memory (P4-07), scheduled jobs (P4-08), connectors and messaging (P4-11) and conversational configuration, `/configure` (P4-09). A passing child proves its parts. What no child proves is the seams: six more overlays that read and write the same Hermes profile through the clients the phase 4a screens use; seven more slash commands in one palette; one set of secret rules for key fields, messaging credentials and `/configure` requests; one gateway; and `/configure`, which applies changes by calling the other children's services, so a defect in any of them shows only there.

The unit does four things.

1. It extends the smoke suite of P1-13 with `--suite phase4b`: a 14-step run that drives the six new overlays and two command-line tools in a real pane, writes nothing, makes no outside call itself (the gateway's model-provider calls for the two ordinary turns are the only outside traffic it causes), and fails with the screen text when something is wrong. The earlier five suites (phase 1, 2a, 2b, 3, 4a) each run once more as regressions.
2. It reruns every user acceptance scenario of the six issues (36) and adds 11 scenarios that only exist when features combine, plus one row for the operator's live write suites.
3. It defines the operator's own check for phase 4 as five concrete rows (OPC-1 to OPC-5). The roadmap's check is: "add a provider key, add an MCP server, change a profile's model and create a cron job without leaving Talaria or opening a YAML file". The key and the model belong to phase 4a, the MCP server and the cron job to phase 4b, so the whole check lives here. One session does all of it, and the last row proves nothing was done outside Talaria and puts every value back.
4. It records the result in two markdown files, checked by a short script (about 60 lines).

The record keeps the ceremony small (the Python client spent 44 percent of its added lines on acceptance records: [02-current-state.md](../02-current-state.md#why-it-ended-up-here), item 4): two markdown files and one checker of about 60 lines, or, under shape B, a table in a pull request (section 10.1). The only other code is the smoke extension, which is a tool, not a record.

## 2. Facts

### 2.1 The `herdr` command line

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) and section 2.1 of [PH4A-ACC](PH4A-ACC.md) apply unchanged (Herdr 0.9.4 when they were checked; re-check with the Herdr in use and write `herdr --version` in the record header). Three bear on this unit: `pane run` types text and Enter together; `pane read` has sources `visible`, `recent` and `recent-unwrapped`; a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. `tmux` is needed in two places only: AUTO-9 (the phase 2a suite) and XF-7 (an 80 by 24 screen).

### 2.2 Hermes and Talaria facts used (re-check at the release in `crates/hermes-contract/PINNED`)

- `prompt.submit` accepts `unattended` only together with `finite: true`, and then auto-approves the turn (`admit_finite` and `finite_turn_scope`, `gateway/session_finite.py` lines 34 to 57 at Hermes v0.22.4; read again while drafting this unit). Nothing here sends it.
- The legacy `methods` list of `apps/shared/src/gateway-contract.openrpc.json` at v0.22.4 names `reload.mcp`, `cron.manage`, `mcp.catalog`, `mcp.servers.list` and `mcp.servers.test` (checked with `jq` while drafting). Whether each is allowed on Talaria's socket ticket is the source issues' business (P4-04 AC-5 and P4-08 AC-13 name the findings); this unit sends none of them from a smoke step.
- `hermes config show` is not a proof that nothing changed, and this unit never runs it as one. The proof is the private byte snapshot `snap` of section 3, as in section 2.2 of [PH4A-ACC](PH4A-ACC.md) (which cites the Hermes lines): the SHA-256 digest of `config.yaml`, `.env` and every other top-level `*.yaml` and `*.md` file of `fixture`'s home, never their text. The cron job store is **outside** `snap`, verified: each profile keeps its jobs in `cron/jobs.json` under its own home (`cron/jobs.py` lines 78 to 90 at Hermes v0.22.4, a top-level object `{"jobs": [...], "updated_at": ...}` written at lines 1530 to 1540), a subfolder `snap` never reads. So a `cmp=0` result never covers cron jobs; wherever a row claims no cron change it also compares `cronids` (section 3) before and after. Whether an MCP server list change or a skill change moves a `snap` digest is still **verify at build time** (section 10.4 item 3); where it does not, the row's own read-back is the proof.
- A running `talaria` started with `--no-ensure` never runs `hermes gateway ensure`. With an empty Hermes home it prints `talaria: no Hermes gateway is running for this home`, a `Next step:` line naming `hermes gateway ensure --json`, and exits 1 (P1-12 UAT-5). So a scenario that launches `talaria` against an empty home without `--no-ensure` would start a gateway; no row here does (section 7.2 ruling 5 and section 10.4 item 1 say what that means for P4-07 UAT-6).
- `reload.mcp` with `confirm: true` reconciles the profile's servers and commits a new MCP generation for the **calling** session only: "other sessions take theirs when next attached" (`reload_mcp` docstring, `gateway/session_ancillary.py` lines 254 to 262; `_commit_session_mcp`, lines 338 to 353, "never another session's"; Hermes v0.22.4). Without `confirm` it returns `{status: "confirm_required", message: "Reloading MCP refreshes every chat of this profile."}` (line 268). So a second chat that is already open adopts a new server only through its own apply (`/mcp apply`, P4-04 REQ-11: `session: applied` shows `MCP servers reloaded. This session's tools change from its next turn.`) or by being reopened. XF-11 and OPC-3 depend on this.
- Ctrl+C clears a non-empty draft (P1-08 settled decision 1) and Esc interrupts a running turn (P1-08).
- `talaria --record <file>` writes the frames a session sends and receives (used by P4-09 UAT-3 and P4-04 UAT-5). The recorder writes no secret by design; XF-3 and XF-5 check that on the new overlays.

### 2.3 What the source issues provide

Each screen text is owned by one source issue and listed with its source in section 9.5. The ignored live tests are named in sections 7.1 and 7.5. The six source issues' final bodies are the authority for every scenario; Appendix A indexes them.

## 3. Freezing and building the candidate

The smoke extension (section 9) is built and merged first, on branch `p4-acc-smoke`, by a test author and a developer; it is part of the candidate. The lead freezes after the six accepted issues, `PH4A-ACC`, P1-13, PH2A-ACC, PH2-ACC and PH3-ACC and the extension are merged and closed, and after the operator's `Ruling D-6:` comment is on the parent (section 10.3). Never switch the primary checkout (it may hold untracked material); work in a worktree outside the repository.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p4-acc                                     # a new folder outside the repository
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p4-acc-candidate"   # for the bench runs of AUTO-1; deleted at closeout
cargo build --release --workspace --examples             # the one release build: these two commands together,
cargo build --release -p talaria-cli -p talaria-bench       # run once at the freeze and never again on this candidate
shasum -a 256 target/release/talaria                       # the binary every row uses
git switch -c p4-acc-record                                # the record branch starts at the candidate commit
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

Helpers every XF and OPC row uses (sections 7.3 to 7.5); they are the helpers of section 3 of [PH4A-ACC](PH4A-ACC.md), unchanged:

```bash
pane_new() { herdr pane split "$HERDR_PANE_ID" --direction right --no-focus --cwd "$1" | jq -r '.result.pane.pane_id'; }
submit()   { herdr pane send-text "$1" "$2"; herdr pane send-keys "$1" enter; }   # two calls: the text, then Enter
vis()      { herdr pane read "$1" --source visible; }
rec()      { herdr pane read "$1" --source recent-unwrapped --lines 3000; }
FH=~/.hermes/profiles/fixture                                                     # the fixture profile's home
plain()    { perl -pe 's/\e\[[0-9;?]*[ -\/]*[@-~]//g; s/\e\][^\a]*\a//g'; }      # strips terminal escape sequences
fleet()    { "$T" fleet list --profile fixture "$@"; }                            # PH3-ACC's helper, for the quiet-fleet check
snap()     { for f in "$FH"/*.yaml "$FH"/*.md "$FH"/.env; do [ -f "$f" ] && printf '%s %s\n' "$(basename "$f")" "$(shasum -a 256 < "$f" | cut -c1-64)"; done; }
cronids()  { if [ -f "$FH/cron/jobs.json" ]; then jq -r '.jobs | if type == "array" then map(.id) else keys end | sort | join(" ")' "$FH/cron/jobs.json"; else echo none; fi; }   # the cron job ids, outside snap
```

`snap` prints one line per file: its name and its digest. It reads only the top-level files of `fixture`'s home, so it never sees the cron store (section 2.2); `cronids` prints the job ids (hex, which name nothing private) so a row can compare them before and after. Its output stays in the row's `$X` folder and is never pasted; only `cmp` results go into evidence. Every captured text goes through `plain` before any `grep`, anchored match, comparison or width count (`vis "$PANE" | plain > "$X/r1.txt"`); the smoke suite does the same inside `screen4b.rs`.

**"The composer is visible".** Right after Talaria starts in a row's pane and shows an empty composer, the tester takes one `vis` read and keeps its last three non-blank lines as `$X/idle-tail.txt`. "The composer is visible" means the last three non-blank lines of a later read equal that file. The operator precondition is that no status-line command is configured. The smoke suite's `idle_screen` uses the same rule against the first read. A baseline is valid only for the pane and terminal size it was taken in: a row that starts Talaria somewhere else (XF-7's tmux session) takes its own.

**Back to the composer.** Before every slash command or prompt that a row sends to Talaria's composer, the tester returns to the composer: while the last read shows an overlay title of section 9.5, a card, a filter line or a confirmation, send `esc`, wait 400 ms and read, at most 4 times. Esc closes a card first, then clears a filter, then closes the overlay (P4-02 REQ-13 and P4-04's filter rule), so a list left filtered needs two presses after its card closes. When the read shows no overlay title and "the composer is visible", type the command. After 4 presses without that, the row FAILs with `could not return to the composer` and the read; a fifth `esc` is never sent. Esc is never sent while a turn is running (it interrupts the turn), except where a row says so. Every step of sections 7.3 and 7.4 that types a slash command after another screen applies this rule first; "back to the composer" in those steps names it.

**`$BASE`.** The `main` commit the smoke extension started from: just before the `p4-acc-smoke` pull request merges, the lead runs `git fetch origin; git merge-base origin/main origin/p4-acc-smoke`, posts the 40-character result on the issue as `BASE=<sha>` and adds the same line to `../acc.env`. The branch is kept until closeout so AC-1 can read it.

`--no-ensure` is on every launch that starts a live session, so Talaria can never start a gateway. Preflight before stage 1 (once): `herdr --version`, `tmux -V`, `jq --version`; run both pane-split forms (the source scenarios' `herdr pane split --current --direction right --cwd . --no-focus` and the helper's) and close the panes they make. A form Herdr rejects is reported on the issue and allowed by REQ-11 change (4). Source scenarios that name a pane size (120 by 40) need the pane or the Herdr window sized to that before they run. If a code path changes after this point, it is a new candidate.

## 4. The record

File `docs/acceptance/v0.7.0/phase-4.md`. The first commit holds this skeleton with every header line filled in with its real value (the checker refuses a header that still holds an angle-bracket placeholder; the values below are examples) and `phase-4-evidence.md` holding only its title line, so the first run of the real checker is the red run: 67 problems. Commit order: the checker tests with the stub (`test(acceptance): add the phase 4 checker tests and a stub`), the empty record (`docs(acceptance): add the empty phase 4 record`), then the real checker (`docs(acceptance): add the phase 4 checker`); at the empty-record commit the stub still prints `FAILED (stub)`, so the 67-problem run is taken at the checker commit, while the record is still all NOT RUN. Row order is the order below; do not add other tables.

```markdown
# Phase 4 acceptance record

Candidate: 0123456789abcdef0123456789abcdef01234567
Hermes: v0.22.4
Herdr: herdr 0.9.4
OS: macos
Date: 2026-12-07

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
| P4-04 UAT-1 | configurer | NOT RUN |  |  |
| P4-04 UAT-2 | developer | NOT RUN |  |  |
| P4-04 UAT-3 | configurer | NOT RUN |  |  |
| P4-04 UAT-4 | configurer | NOT RUN |  |  |
| P4-04 UAT-5 | configurer | NOT RUN |  |  |
| P4-04 UAT-6 | configurer | NOT RUN |  |  |
| P4-05 UAT-1 | configurer | NOT RUN |  |  |
| P4-05 UAT-2 | configurer | NOT RUN |  |  |
| P4-05 UAT-3 | configurer | NOT RUN |  |  |
| P4-05 UAT-4 | configurer | NOT RUN |  |  |
| P4-05 UAT-5 | configurer | NOT RUN |  |  |
| P4-05 UAT-6 | scripter | NOT RUN |  |  |
| P4-07 UAT-1 | configurer | NOT RUN |  |  |
| P4-07 UAT-2 | developer | NOT RUN |  |  |
| P4-07 UAT-3 | configurer | NOT RUN |  |  |
| P4-07 UAT-4 | configurer | NOT RUN |  |  |
| P4-07 UAT-5 | configurer | NOT RUN |  |  |
| P4-07 UAT-6 | developer | NOT RUN |  |  |
| P4-08 UAT-1 | fleet-lead | NOT RUN |  |  |
| P4-08 UAT-2 | configurer | NOT RUN |  |  |
| P4-08 UAT-3 | fleet-lead | NOT RUN |  |  |
| P4-08 UAT-4 | configurer | NOT RUN |  |  |
| P4-08 UAT-5 | fleet-lead | NOT RUN |  |  |
| P4-08 UAT-6 | configurer | NOT RUN |  |  |
| P4-11 UAT-1 | configurer | NOT RUN |  |  |
| P4-11 UAT-2 | configurer | NOT RUN |  |  |
| P4-11 UAT-3 | configurer | NOT RUN |  |  |
| P4-11 UAT-4 | configurer | NOT RUN |  |  |
| P4-11 UAT-5 | configurer | NOT RUN |  |  |
| P4-11 UAT-6 | configurer | NOT RUN |  |  |
| P4-09 UAT-1 | configurer | NOT RUN |  |  |
| P4-09 UAT-2 | developer | NOT RUN |  |  |
| P4-09 UAT-3 | configurer | NOT RUN |  |  |
| P4-09 UAT-4 | fleet-lead | NOT RUN |  |  |
| P4-09 UAT-5 | scripter | NOT RUN |  |  |
| P4-09 UAT-6 | configurer | NOT RUN |  |  |
| XF-1 | developer | NOT RUN |  |  |
| XF-2 | configurer | NOT RUN |  |  |
| XF-3 | developer | NOT RUN |  |  |
| XF-4 | scripter | NOT RUN |  |  |
| XF-5 | developer | NOT RUN |  |  |
| XF-6 | developer | NOT RUN |  |  |
| XF-7 | developer | NOT RUN |  |  |
| XF-8 | developer | NOT RUN |  |  |
| XF-9 | configurer | NOT RUN |  |  |
| XF-10 | configurer | NOT RUN |  |  |
| XF-11 | configurer | NOT RUN |  |  |
| OPC-1 | configurer | NOT RUN |  |  |
| OPC-2 | configurer | NOT RUN |  |  |
| OPC-3 | configurer | NOT RUN |  |  |
| OPC-4 | configurer | NOT RUN |  |  |
| OPC-5 | configurer | NOT RUN |  |  |
| OP-1 | configurer | NOT RUN |  |  |
```

Header values: `Candidate:` is `$CANDIDATE`; `Hermes:` is the `tag=` value of `crates/hermes-contract/PINNED` at the candidate (`sed -n 's/^tag=//p' crates/hermes-contract/PINNED`; the file is exactly two lines, `tag=<tag>` and `commit=<40 hex>`, per P0-03); `Herdr:` is the whole first line `herdr --version` prints, verbatim; `OS:` is `macos` or `linux`; `Date:` is the day the record is completed.

Rules: `result` is exactly `PASS`, `FAIL`, `BLOCKED (<cause>)` (causes: `environment`, `not built`, `no access`) or `NOT RUN`. A PASS row carries the first 7 or more characters of the candidate in `commit` and the link `phase-4-evidence.md#<id lower case, spaces to hyphens>` in `evidence`; the row `P4-08 UAT-2` links `phase-4-evidence.md#p4-08-uat-2`. A row that is not PASS may leave `evidence` and `commit` empty but its evidence section should say why. BLOCKED and NOT RUN never count as a pass; there is no waiver state. Personas are those of the source scenarios (Appendix A); AUTO rows are `builder`; OPC rows are `configurer`.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-4-evidence.md` starts with `# Phase 4 acceptance evidence` and holds one section per row, in record order:

````markdown
## P4-08 UAT-1

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-12-07.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

A row run more than once keeps one `## <id>` section; each run adds its own line and blocks under it, oldest first, and the last run is the row's result:

````markdown
## XF-6

Candidate: 0123456. Runner: agent tester. Run 1 of 2. Date: 2026-12-07. Not a Talaria defect: the provider answered 429 (rerun approved on the issue).

```text
<run 1 output>
```

Candidate: 0123456. Runner: agent tester. Run 2 of 2. Date: 2026-12-07.

```text
<run 2 output>
```
````

The checker requires, for every PASS row, the `Candidate:` line (7 or more hex characters equal to the start of the candidate, then a full stop or space), exactly one section, and a closed fenced block with at least one non-blank line and at most 60 lines (a fence left open at the end of the file is a problem). The checker reads the last `Candidate:` line of a section and cannot count runs. The reviewer checks what the script cannot: the runner and date line, the run list (`Run <k> of <n>` lines numbered 1 to n), the three-run limit, the operator's comment URL, the row order and persona column, and that every row that is not PASS says why. An operator row holds the operator's words (what they did, the terminal, the result), the URL of the operator's comment on the issue and the read-back of each value they restored.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output (the result lines). Source rows: what the source scenario's Evidence line asks for. XF and OPC rows: what sections 7.3 and 7.4 name. Smoke rows (AUTO-8 to AUTO-14): the whole output including every step line and the `SMOKE_RESULT:` line, with repeated `NOTE` lines removed (the only lines ever removed), and both tab lists. When the scrubbed output is still over 60 lines, write it as consecutive fenced blocks of at most 60 lines each in the same section, in order, none cut in the middle of a line: the checker measures each block, not the section, and AC-10 reads the `SMOKE_RESULT:` line from whichever block holds it. Screenshots are attached to the operator's comment, not committed. Configuration text is evidence only as the lines a source scenario names; never paste a whole file, a `snap` digest or `hermes config show` output.

### 5.3 Scrubbing (before anything is pasted)

```bash
scrub() {
  sed -e "s#$HOME#<home>#g" -e "s#$(id -un)#<user>#g" \
      -e "s#${TMPDIR%/}#<tmp>#g" -e "s#$(cd "${TMPDIR:-/tmp}" && pwd -P)#<tmp>#g" \
      -e "s#$PWD#<worktree>#g"
}
herdr pane read "$PANE" --source recent-unwrapped --lines 200 | scrub > "$X/evidence.txt"
```

Then read the result: no home directory, user name (a login name under four characters cannot be scrubbed safely: say so in the section), temporary path, host name, provider key, ticket or key tail beyond the four characters the screen itself shows, and no profile name other than `default` or `fixture`; replace any other with `<profile>`. Shapes that are new in phase 4b: an MCP server's URL (the loopback address and port of P4-04 UAT-3 and UAT-5: write them `<loopback>`), the label or account name of a connector or a messaging pairing (P4-11 asks for `<redacted>`), the cron job ids (hex: keep them, they name nothing private), a skill hub or plugin repository URL (public; keep), and the operator's provider account in OPC-1 (the key tail the screen shows is the only part of a key that may appear). The checker catches paths only; AC-11 also greps `--profile` values, key shapes and each runner's login name. A clean grep proves only that those shapes are absent: the reading step is the control.

## 6. The checker and its tests

### 6.1 Behaviour

`check-phase-4.sh [record [evidence]]` (defaults: `phase-4.md` and `phase-4-evidence.md` next to the script). It holds the 67 row ids in one `IDS=` line. It prints one line per problem, then `ACCEPTANCE_RESULT: PASSED (<n> of 67 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 67 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. Problems: a row that is not PASS (the line is `<id>: <result>`); a result that is not one of the four forms; a PASS row whose commit is not a prefix of the candidate or is shorter than 7 characters; a PASS row whose evidence link is not the expected one, whose section is missing, empty or longer than 60 lines, or does not name the candidate; a missing, extra or duplicate row; a section that appears twice; a header line that is missing, empty or still a placeholder; an evidence file that ends inside an unclosed fenced block (text in it never counts); a local path anywhere in either file.

The script is a copy of the phase 4a checker (itself a copy of PH2A-ACC's) with the file names, the id list and the count changed, so the phases stay comparable. Rejected: one parametrised script for all phases, because it would reopen a closed gate's tool. If a command-saving proxy wraps shell commands on the machine, run the checker and its tests through the proxy's pass-through mode or in a plain pane and paste the raw output.

### 6.2 Reference implementation

Written for POSIX `sh` and `awk`; no brace counts in regular expressions; the backtick fence written `\140\140\140`. Exercised while drafting on macOS (and again after the review repair below): the 26 cases below pass, the stub fails every one of them, and the skeleton record of section 4 ends `FAILED (67 problems, 0 of 67 rows PASS)` with 68 lines.

Review repair, verified by running it: the first version compared row and evidence commits with the candidate only once a valid `Candidate:` line had already been read, so a record whose valid header sat **after** the table was accepted with every row on commit `deadbee` (exit 0, `PASSED (67 of 67 ...)`). The `Candidate:` line is now accepted only once and only above the table (a later or second one is a problem), and a PASS row or evidence section is never accepted without a valid candidate. It also counted a section as good when any of its blocks held text; now every closed block of a PASS row's section must hold a non-blank line (case 25). The old version passed both new cases; this one fails them for the named reason. Not run with GNU `awk` or `mawk`: run the cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 4 acceptance record. Usage: check-phase-4.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-4.md}
EVI=${2:-$DIR/phase-4-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-phase-4: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|AUTO-8|AUTO-9|AUTO-10|AUTO-11|AUTO-12|AUTO-13|AUTO-14|P4-04 UAT-1|P4-04 UAT-2|P4-04 UAT-3|P4-04 UAT-4|P4-04 UAT-5|P4-04 UAT-6|P4-05 UAT-1|P4-05 UAT-2|P4-05 UAT-3|P4-05 UAT-4|P4-05 UAT-5|P4-05 UAT-6|P4-07 UAT-1|P4-07 UAT-2|P4-07 UAT-3|P4-07 UAT-4|P4-07 UAT-5|P4-07 UAT-6|P4-08 UAT-1|P4-08 UAT-2|P4-08 UAT-3|P4-08 UAT-4|P4-08 UAT-5|P4-08 UAT-6|P4-11 UAT-1|P4-11 UAT-2|P4-11 UAT-3|P4-11 UAT-4|P4-11 UAT-5|P4-11 UAT-6|P4-09 UAT-1|P4-09 UAT-2|P4-09 UAT-3|P4-09 UAT-4|P4-09 UAT-5|P4-09 UAT-6|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|XF-9|XF-10|XF-11|OPC-1|OPC-2|OPC-3|OPC-4|OPC-5|OP-1'
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
    else if (cur != "" && line ~ /^Candidate: /) { split(trim(substr(line, 12)), t, /[. ]/); evcand[cur] = t[1] }
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
  if (evid != "phase-4-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-4-evidence.md#" slug(id))
  if (!(id in sect) || text[id] < 1) bad(id ": the evidence file has no section with a fenced block holding text for it")
  else if (longest[id] > 60) bad(id ": a fenced block of the evidence is longer than 60 lines")
  else if (blank[id] > 0) bad(id ": a fenced block of the evidence holds no text")
  if (id in sect && (!iscand(cand) || length(evcand[id]) < 7 || index(cand, evcand[id]) != 1)) bad(id ": the evidence section must name the candidate (Candidate: <7 hex>.)")
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

26 cases. The test script builds a good record and variations of it in a temporary folder, runs the checker on each and compares the exit code and one expected text. It prints `CASE <name>: ok` or `CASE <name>: FAILED (...)` and a final `CHECK_RECORD_TESTS: PASSED` or `CHECK_RECORD_TESTS: FAILED (<n>)`. It reads the id list out of the checker, so the two cannot drift; `all-pass` expects the text `67 of 67`, so a dropped id fails it.

| # | Case | Builds | Expects (exit; text) |
|---|---|---|---|
| 1 | `all-pass` | 67 PASS rows, a section each | 0; `ACCEPTANCE_RESULT: PASSED (67 of 67 rows PASS, candidate <7 hex>)` |
| 2 | `not-run-fails` | `P4-09 UAT-6` NOT RUN | 1; `P4-09 UAT-6: NOT RUN` |
| 3 | `blocked-fails` | `AUTO-5` `BLOCKED (environment)` | 1; `AUTO-5: BLOCKED (environment)` |
| 4 | `fail-fails` | `XF-2` FAIL | 1; `XF-2: FAIL` |
| 5 | `wrong-commit` | `P4-08 UAT-2` commit `deadbeefdead` | 1; `is not the candidate` |
| 6 | `lower-case-result` | `AUTO-1` result `pass` | 1; `the result must be` |
| 7 | `row-missing` | `OP-1` deleted | 1; `OP-1: row missing` |
| 8 | `unexpected-row` | an added row `ZZ-9` | 1; `unexpected row: ZZ-9` |
| 9 | `duplicate-row` | `AUTO-1` twice | 1; `AUTO-1: appears twice` |
| 10 | `no-candidate` | `Candidate: TBD` | 1; `Candidate: <40 hex` |
| 11 | `no-evidence-section` | no section for `P4-11 UAT-6` | 1; `P4-11 UAT-6: the evidence file has no section` |
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
| 22 | `empty-header-value` | `Herdr: ` with nothing after it | 1; `Herdr: the header line has no value` |
| 23 | `unclosed-fence` | the last section (`OP-1`) loses its closing fence | 1; `the evidence file ends inside an unclosed fenced block` |
| 24 | `late-candidate-header` | the valid `Candidate:` line moved below the table, every row and evidence section on `deadbee` | 1; `the line Candidate: must appear once, above the table` |
| 25 | `empty-block-after-text` | `AUTO-2` has a block with text and then a blank-only block | 1; `AUTO-2: a fenced block of the evidence holds no text` |
| 26 | `default-arguments` | the two files next to the script, no arguments | 0; `ACCEPTANCE_RESULT: PASSED` |

The checker ignores heading-like lines inside a fenced block (case 1 builds one in `XF-6`'s block) and counts a duplicated section. The stub the test author commits first is

```sh
#!/bin/sh
# stub: the real checker replaces this file
IDS='<the same IDS line as in the reference checker>'
echo "ACCEPTANCE_RESULT: FAILED (stub)"; exit 1
```

so every case fails on its expectation (a stub without the `IDS=` line would make every case build an empty record and fail for the wrong reason). Copying the reference test script below is allowed and expected; independence means the author is not the developer who writes the checker.

````sh
#!/bin/sh
# Tests for check-phase-4.sh. Run: sh docs/acceptance/v0.7.0/check-phase-4.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-4.sh"
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
      echo "| $id | developer | $res | phase-4-evidence.md#$slug | $com |"
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
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (67 of 67 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P4-09 UAT-6" "NOT RUN";               expect not-run-fails 1 "P4-09 UAT-6: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P4-08 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-1' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-1: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P4-11 UAT-6";               expect no-evidence-section 1 "P4-11 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-4-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
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
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-4.md" && cp "$T/ev.md" "$T/d/phase-4-evidence.md"
out=$(sh "$T/d/check-phase-4.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record, taken at the commit that adds the real checker while the committed record is still the skeleton of section 4 with the real candidate and real header lines: `sh check-phase-4.sh` must end `ACCEPTANCE_RESULT: FAILED (67 problems, 0 of 67 rows PASS)` and exit 1 (checked while drafting).

## 7. The scenario catalogue

Every row has an id, a persona, a runner, steps, an observable result and the evidence to capture. Runner words: `agent tester` is an agent working in a Herdr pane; `operator` is the human. All commands run at the worktree root unless a row says otherwise. `R=infiquetra/talaria`, `X=$(mktemp -d)` for evidence scratch. No agent row sets `TALARIA_PROBE_WRITES`, `TALARIA_PROBE_CONNECTORS` or `TALARIA_CRON_FREE_ROUTE`, passes `--restart`, signs in, creates, stops or starts a profile or the gateway, saves a setting or a key, or runs `hermes config set`, `hermes update` or `hermes gateway` anything.

### 7.1 AUTO rows (persona builder; agent tester)

**AUTO-1: project check and the candidate's continuous integration (CI) results.** As AUTO-1 of [PH4A-ACC](PH4A-ACC.md) with `p4-acc-candidate` in place of `p4a-acc-candidate`. The `bench` workflow runs on `pull_request` and `workflow_dispatch` (P1-11). Verified at first use: a dispatch run on the candidate branch should report a check run on the candidate commit. The lead learns the answer in stage 0 by dispatching `bench.yml` on `p4-acc-smoke` and reading `gh api "repos/$R/commits/<its head sha>/check-runs"`.

```bash
gh workflow run bench.yml --ref p4-acc-candidate -R "$R"
# poll every 10 s, at most 5 minutes, until a run whose headSha is the candidate exists (an older run on the branch does not count)
RUN=$(gh run list -R "$R" --workflow bench.yml --branch p4-acc-candidate --json databaseId,headSha --jq ".[] | select(.headSha == \"$CANDIDATE\") | .databaseId" | head -1)
gh run watch "$RUN" -R "$R" --exit-status
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
gh api --paginate "repos/$R/commits/$CANDIDATE/check-runs" --jq '.check_runs[] | "\(.name) \(.conclusion)"'
shasum -a 256 target/release/talaria
```

If no run whose `headSha` is the candidate appears within 5 minutes, poll once more for 5 minutes; if there is still none, AUTO-1 is `BLOCKED (environment: no bench run for the candidate)`, reported on P1-11 (section 8.1 item 3); no candidate is accepted with it BLOCKED. PASS: `gh run watch` exits 0 on the run for the candidate; the cargo set exits 0; every check-run line ends `success` or `skipped` (`null` is still running: wait and read again; `failure`, `cancelled`, `timed_out` and `action_required` are a FAIL) and `bench success` is among them. Evidence: run id and `headSha`, exit codes, the check-run lines, the checksum.

**AUTO-2: wire and recorded-response suites** (tests that check requests and replies against the pinned contract or recorded responses).

```bash
cargo test -p hermes-client --test mcp_api --test capabilities_api --test memory_api --test cron_api --test messaging_api
cargo test -p hermes-contract --test reload_mcp_override
cargo test -p talaria-app --test capabilities_replay --test configure_wire
```

PASS: each exits 0 and prints at least k lines `test result: ok. N passed` with N at least 1 for the k targets it names (a target that ran no test prints `0 passed` and is a FAIL). A target that does not exist at the candidate is `BLOCKED (not built)`. Evidence: the `test result` lines.

**AUTO-3: the six issues' unit suites** (every test target the issues name, except the wire, replay and live ones above and below).

```bash
cargo test -p talaria-core --test mcp_servers --test mcp_form --test mcp_catalogue --test mcp_signin --test mcp_apply
cargo test -p talaria-core --test capabilities_text --test capabilities_rows --test capabilities_state --test capabilities_props
cargo test -p talaria-core --test memory_view --test memory_fields --test memory_state --test memory_command
cargo test -p talaria-core --test cron_time --test cron_list --test cron_form --test cron_run_now --test cron_history --test cron_props
cargo test -p talaria-core --test connectors_rows --test connectors_accounts --test connectors_link --test connectors_rules \
  --test messaging_rows --test messaging_edit --test messaging_pairing --test connectors_props
cargo test -p talaria-core --test configure_reply --test configure_validate --test configure_brief --test configure_state --test configure_props
cargo test -p talaria-app --test mcp_service --test memory_service --test cron_service --test connectors_service --test configure_service
cargo test -p talaria-app --test capabilities_service
cargo test -p talaria-tui --test mcp_render --test mcp_props --test memory_render --test cron_render --test connectors_render --test configure_render
cargo test --release -p talaria-tui --test capabilities_render --test capabilities_keys
cargo test -p talaria-cli --test mcp_wiring --test capabilities_cmd --test configure_cmd
cargo test -p talaria-cli --test live_cron run_verdict
```

PASS: every command exits 0, and a command naming k `--test` targets prints at least k lines `test result: ok. N passed` with N at least 1. A target that no longer exists is `BLOCKED (not built)`. If a source issue renamed a target before the candidate, that issue's current Verification block wins and the difference is reported here. Evidence: the `test result` lines and exit codes. Tests marked `#[ignore]` are run by AUTO-5, by the source rows or by OP-1.

**AUTO-4: performance budgets on the units' own benchmarks and the real release binary.**

```bash
target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"; echo "exit=$?"
grep -cE ' FAIL$' "$X/bench.txt"; grep -cE ' PASS$' "$X/bench.txt"
cargo bench -p talaria-core --bench configure_brief
cargo test --release -p talaria-tui --test cron_render fifty_jobs_within_budget
```

PASS: `exit=0`, the FAIL count 0, the PASS count 1 or more; the bench exits 0 and reports a median of 10 ms or less (P4-09 AC-13; the tester quotes the median line); the test passes (P4-08 AC-8). The other new screens' budgets (P4-04's 50 ms, the idle rules) are asserted inside the suites of AUTO-3. The flags are P1-11's. Evidence: the `bench.txt` table from its `os:` line, the two counts, the median line, the exit codes. Budgets: [05-architecture.md](../05-architecture.md#performance-budgets).

**AUTO-5: live read-only suites with no skip.** Operator preconditions hold; each command runs in its own shell and none writes.

```bash
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_mcp -- --ignored --nocapture --test-threads=1 \
  live_reads_on_fixture live_reload_gate_has_no_side_effects > "$X/mcp.txt" 2>&1; echo "mcp exit=$?"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_capabilities -- --ignored --nocapture live_capabilities_read > "$X/cap.txt" 2>&1; echo "cap exit=$?"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_memory reads_memory_and_curator_shapes -- --ignored --nocapture > "$X/mem.txt" 2>&1; echo "mem exit=$?"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_cron -- --ignored --nocapture live_reads_on_fixture > "$X/cron.txt" 2>&1; echo "cron exit=$?"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_connectors -- --ignored --nocapture live_messaging_reads_on_fixture > "$X/msg.txt" 2>&1; echo "msg exit=$?"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_configure -- --ignored --nocapture \
  live_session_is_locked_down live_dry_run_proposes_without_writing > "$X/cfg.txt" 2>&1; echo "cfg exit=$?"
cat "$X/mcp.txt" "$X/cap.txt" "$X/mem.txt" "$X/cron.txt" "$X/msg.txt" "$X/cfg.txt"   # displayed after the statuses were saved
```

Each test's output goes to its file first and its exit status is printed straight after, so a test that prints its signal and then fails (a later assertion, a panic, cleanup) cannot hide behind a pipe: a pipe into `tee` reports `tee`'s status, not the test's. PASS (all of these):  the six `exit=` lines each print 0; each file's `test result: ok. N passed` line has N equal to the number of tests the command names (2, 1, 1, 1, 1, 2); and each test has its own signal: `LIVE_MCP_RESULT: PASSED` twice, `LIVE_CAPABILITIES_RESULT: PASSED`, `LIVE_MEMORY_RESULT: PASSED`, `LIVE_CRON_RESULT: PASSED` and `LIVE_CONNECTORS_RESULT: PASSED` once each, `LIVE_CONFIGURE_RESULT: PASSED` twice (`grep -c` over each file), and no `SKIPPED` anywhere. A skip is `BLOCKED (environment)`, never a pass. The two `live_configure` tests start a hidden Hermes session on the free model and write nothing. The write variants and the connectors gate are OP-1, the operator's. Evidence: the six `exit=` lines, the `test result` lines, the signal lines and counts.

**AUTO-6: hygiene scans.** Every path below is a source issue's; a path that does not exist at the candidate is `BLOCKED (not built)`, not a pass.

```bash
cargo run -q -p record-fixture -- scan fixtures/frames
! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools
! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input*.rs' ':!tools/smoke/README.md'
! git grep -nE 'TALARIA_PROBE_|--restart' -- tools/smoke ':!tools/smoke/tests/typed_input*.rs' ':!tools/smoke/README.md'
! grep -rnwE 'force|accept_capabilities|unattended|tools\.configure|tools\.list|toolsets\.list|reload\.mcp|hub/install' crates/talaria-core/src/capabilities crates/hermes-client/src/capabilities_api.rs crates/talaria-app/src/capabilities.rs crates/talaria-tui/src/capabilities crates/talaria-cli/src/capabilities_cmd.rs
! grep -rE '/(Users|home|private|var/folders|tmp)/' fixtures/capabilities
! git grep -n -w 'unattended' -- crates/talaria-core/src/cron crates/talaria-app/src/cron.rs crates/hermes-client/src/cron_api.rs crates/talaria-tui/src/cron
! grep -rnE '"(unattended|api_text|yolo|title|skills)"' crates/talaria-app/src/configure/
! grep -rn 'hermes_client' crates/talaria-core/src/configure/
SECRET='sk-[A-Za-z0-9]{8,}|sk-or-v1-[0-9a-f]{8,}|Bearer '
printf '%s\n' 'sk-abcdefgh12345678' | grep -cE "$SECRET"; printf '%s\n' 'sk-or-v1-0000000000' | grep -cE "$SECRET"; printf '%s\n' 'Authorization: Bearer x' | grep -cE "$SECRET"
! git grep -nE "$SECRET" -- fixtures tools crates
! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile|nosuch'
```

PASS: the `!` lines print nothing; every other line exits 0. The three positive controls each print 1 before the clean scan counts: the pattern is the same as AC-11's, and `sk-[A-Za-z0-9]{8,}` alone misses `sk-or-v1-...` (the hyphen after `or` ends the match). A control that prints 0 makes the row FAIL whatever the scan shows. The sentinel texts of the source scenarios (`sk-or-v1-talaria-sentinel-0000000000000000`, `tk-sentinel-4f1d`, `ev-sentinel-9a2c`, `uat-sentinel-5e2a`) are not hex strings and match none of the patterns; if a source issue's tests hold them in fixtures, that is allowed by REQ-17. A finding is a FAIL reported on this issue and a new candidate follows. Evidence: for each scan, its label (the first word of the command, or the pattern's purpose such as `no unsafe`) and its result (`clean` or the scrubbed finding lines); for the three positive controls, only the labels `control sk`, `control sk-or-v1` and `control bearer` with their counts (`1`). The control values and the `SECRET=` assignment are never pasted: they hold the very shapes AC-11 forbids anywhere in the evidence.

**AUTO-7: smoke suite offline tests.**

```bash
cargo test -p smoke
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
cargo run -q -p smoke -- run --help
B=$(mktemp -d)/base; git worktree add "$B" "$BASE"                  # the baseline: the same list at $BASE (section 3)
(cd "$B" && cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$') > "$X/ignored-base.txt"; git worktree remove --force "$B"
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$' > "$X/ignored-now.txt"
sort "$X/ignored-base.txt" > "$X/b.txt"; sort "$X/ignored-now.txt" > "$X/n.txt"
comm -13 "$X/b.txt" "$X/n.txt"; echo "added=$(comm -13 "$X/b.txt" "$X/n.txt" | wc -l | tr -d ' ')"; echo "removed=$(comm -23 "$X/b.txt" "$X/n.txt" | wc -l | tr -d ' ')"
```

PASS: all tests pass; the ignored list names `live_full_smoke_in_herdr`, `live_phase4a_smoke_in_herdr` and `live_phase4b_smoke_in_herdr` among the earlier phases' ignored tests; `added=1`, and the one added line ends `live_phase4b_smoke_in_herdr: test` (with its module path before it); `removed=0`. (`comm` prints only records, so there is no change header to allow for; an ordinary `diff` would also print a line such as `2a3`.) the help lists `--talaria`, `--herdr`, `--profile`, `--model`, `--provider`, `--deadline-secs`, `--step-scale`, `--report-dir` and `--suite` with `phase4b` among its values. Evidence: the `test result` line, the ignored lines, the help.

**AUTO-8 to AUTO-14: the smoke suites, back to back at the end of stage 1** with no other live scenario between them (they share one gateway and one free model). The five earlier suites run once each as regression rows with their owners' preconditions, step counts, deadlines and no-skip rules (P1-13, PH2A-ACC, PH2-ACC, PH3-ACC, PH4A-ACC); the phase 3 suite also needs PH3-ACC's quiet-fleet check before AUTO-11 and before each later suite, because the last act of a phase 3 run is a denied approval whose turn may still be working. The check, restated from section 7.1 of [PH4A-ACC](PH4A-ACC.md): run `fleet | plain` and read it; it must show no `NEEDS YOU` group and no `WORKING` group. If one is there, run it again every 15 s for up to 5 minutes. A session that still needs an answer is answered with deny (resume it with `"$T" --resume <id> --profile fixture --no-ensure` and press Esc on the card) and the row's evidence says so; a session still `WORKING` after 5 minutes is a stop: report it on the issue. The check is a wait, not a scenario.

```bash
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
snap > "$X/before.sum"; cronids > "$X/cron-before.txt"
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria "$T" > "$X/s1.txt" 2>&1; echo "exit=$?"                        # AUTO-8
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s2.txt" 2>&1; echo "exit=$?"    # AUTO-9
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2b --talaria "$T" > "$X/s3.txt" 2>&1; echo "exit=$?"    # AUTO-10
# quiet-fleet check
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase3 --talaria "$T" > "$X/s4.txt" 2>&1; echo "exit=$?"     # AUTO-11
# quiet-fleet check
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase4a --talaria "$T" > "$X/s5.txt" 2>&1; echo "exit=$?"    # AUTO-12
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase4b --talaria "$T" > "$X/s6.txt" 2>&1; echo "exit=$?"    # AUTO-13
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase4b --talaria "$T" > "$X/s7.txt" 2>&1; echo "exit=$?"    # AUTO-14
herdr tab list --workspace "$HERDR_WORKSPACE_ID"; tmux ls 2>&1 | grep -c talaria-smoke
snap > "$X/after.sum"; cmp "$X/before.sum" "$X/after.sum"; echo "cmp=$?"
cronids > "$X/cron-after.txt"; cmp "$X/cron-before.txt" "$X/cron-after.txt"; echo "croncmp=$?"
```

PASS: AUTO-8 prints six `PASS` lines and `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 300; AUTO-9 prints 21 with n under 1800; AUTO-10 prints 15 with n under 1500; AUTO-11 prints 12 with n under 1200; AUTO-12 prints 17 with n under 900; AUTO-13 and AUTO-14 each print 14 `PASS` lines with n under 900; every `exit=0`; no `talaria-smoke-` tab in the second list, the tmux count 0, `cmp=0` (no top-level configuration, key or memory file of `fixture` changed) and `croncmp=0` (the same cron job ids). The phase 4b suite has no skip path at all (section 9.4): a phase4b run that prints `SKIP` or `PASSED WITH SKIPS` is a FAIL (a defect of the suite), never BLOCKED. If an earlier suite's owner changed a count or a limit in its own issue before this candidate, that issue's number wins and the difference is reported here. The defaults are P1-13's (profile `fixture`, model `minimax-m3.1-flash`, provider `ollama-cloud`), so no such flag is passed. A suite prints `PASSED WITH SKIPS` and exits 3 when a step skips, and with `TALARIA_LIVE_REQUIRE=1` a skip fails the run; neither is a pass: the row is BLOCKED (environment) by section 8.1 item 1. The phase 2a suite's `palette` step checks that the Talaria group lists the local commands; phase 4a added five and this unit's children add seven (section 10.4 says what to do if that step fails). A `cmp=1` is a FAIL of AUTO-12, AUTO-13 and AUTO-14 (those suites must write nothing) unless the tester shows, by rerunning only the earlier suites with snapshots around each, that an earlier suite caused it; then that suite's row is the FAIL, reported to its owner. Evidence: each whole output (section 5.2), both tab lists and the `cmp=` line (never a digest).

### 7.2 Source rows (36 scenarios)

Each source scenario is rerun exactly as its issue writes it (the issue's own text at the time of the run wins; Appendix A indexes them). Who runs it:

| Row | Persona | Runner | Source runner line | Covers |
|---|---|---|---|---|
| P4-04 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-1, AC-3, AC-7, AC-10 |
| P4-04 UAT-2 | developer | operator | operator, or the agent tester with the operator's written opt-in | AC-1, AC-3, AC-5, AC-10 |
| P4-04 UAT-3 | configurer | operator | agent tester with the operator's written opt-in (it writes `fixture`) | AC-1, AC-2, AC-10 |
| P4-04 UAT-4 | configurer | operator | operator (their own account) | AC-4, AC-10 |
| P4-04 UAT-5 | configurer | operator | agent tester with the opt-in; the operator cleans the key file | AC-2, AC-8, AC-10 |
| P4-04 UAT-6 | configurer | operator | operator (SSH) | AC-4, AC-10 |
| P4-05 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-1, AC-4, AC-7 |
| P4-05 UAT-2 | configurer | operator | operator (it writes `fixture`) | AC-2, AC-8 |
| P4-05 UAT-3 | configurer | operator | operator (installs into `fixture`) | AC-3, AC-8 |
| P4-05 UAT-4 | configurer | operator | operator (installs and removes a plugin) | AC-3, AC-8 |
| P4-05 UAT-5 | configurer | agent tester | agent tester in Herdr (sends nothing) | AC-3, AC-5 |
| P4-05 UAT-6 | scripter | agent tester | agent tester in Herdr | AC-4 |
| P4-07 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-9 |
| P4-07 UAT-2 | developer | agent tester | agent tester in Herdr | AC-10 |
| P4-07 UAT-3 | configurer | agent tester | agent tester in Herdr (cancelled delete, refused switch) | AC-11 |
| P4-07 UAT-4 | configurer | operator | operator, or an agent tester with the written go-ahead | AC-12 |
| P4-07 UAT-5 | configurer | operator | operator (deletes `USER.md` on `fixture`) | AC-13 |
| P4-07 UAT-6 | developer | agent tester | agent tester in Herdr (see ruling 5 below) | AC-14 |
| P4-08 UAT-1 | fleet-lead | agent tester | agent tester in Herdr | AC-2, AC-8, AC-11 |
| P4-08 UAT-2 | configurer | operator | agent tester in Herdr; the issue says UAT-2 to UAT-6 write `fixture` and need the written opt-in | AC-2, AC-3, AC-8, AC-11 |
| P4-08 UAT-3 | fleet-lead | operator | either (one agent turn on the free model); needs the free cron route | AC-4, AC-5, AC-8, AC-11 |
| P4-08 UAT-4 | configurer | operator | agent tester in Herdr; a defect in the refusal would create a job | AC-3, AC-11 |
| P4-08 UAT-5 | fleet-lead | operator | agent tester in Herdr; a defect would run a paused job | AC-4, AC-11 |
| P4-08 UAT-6 | configurer | operator | agent tester in Herdr; it creates and removes a job | AC-5, AC-11 |
| P4-11 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-5, AC-8, AC-12 |
| P4-11 UAT-2 | configurer | agent tester | agent tester in Herdr (negative; recorder on) | AC-1, AC-12 |
| P4-11 UAT-3 | configurer | operator | agent tester with the opt-in (it writes `fixture`'s key file) | AC-6, AC-7, AC-9, AC-12 |
| P4-11 UAT-4 | configurer | operator | operator (their Nous sign-in) | AC-2, AC-3, AC-9, AC-12 |
| P4-11 UAT-5 | configurer | operator | operator | AC-3, AC-12 |
| P4-11 UAT-6 | configurer | operator | operator (a connected platform, a second person pairing) | AC-4, AC-10, AC-12 |
| P4-09 UAT-1 | configurer | operator | operator, or the agent tester with the opt-in (it applies a change) | AC-4, AC-5, AC-12 |
| P4-09 UAT-2 | developer | operator | operator, or the agent tester with the opt-in | AC-5, AC-7, AC-12 |
| P4-09 UAT-3 | configurer | agent tester | agent tester in Herdr (negative; recorder on) | AC-2, AC-10, AC-12 |
| P4-09 UAT-4 | fleet-lead | agent tester | agent tester in Herdr (negative; cancel) | AC-4, AC-10, AC-12 |
| P4-09 UAT-5 | scripter | agent tester | agent tester in Herdr (a dry run) | AC-9, AC-12 |
| P4-09 UAT-6 | configurer | agent tester | either (refine and discard; applies nothing) | AC-5, AC-12 |

The operator runs the 21 rows that write `fixture`, save a key, sign in, use an account of theirs or reach a state an agent may not create; an agent tester runs the other 15. "Operator, or an agent tester with the opt-in" means the operator here, as in phase 4a, so the opt-in `TALARIA_PROBE_WRITES=1` exists only in the operator's shell.

Order for the 15 agent rows (stage 2): issue order (P4-04, P4-05, P4-07, P4-08, P4-11, P4-09), one live scenario at a time. P4-05 UAT-5 is run before the operator installs anything (its source says "before UAT-3's install or after its cleanup"). P4-09 UAT-6 applies nothing: its last keys discard the proposal. P4-09 UAT-3 and UAT-4 start a hidden Hermes session that cannot change anything; UAT-3's first half is purely local. Every agent row allows three attempts where the source says so (a free model can misread a plain prompt) and records the number used.

The four allowed changes of the issue's REQ-11, made exact: (1) every `talaria`, `<talaria>` or `"$T"` the scenario runs is written `"$T"`, always (the worktree's `PATH` entry is only a backstop); (2) `--no-ensure` is added to every launch of the interactive screen that lacks it (`talaria` with or without `-c`, `--resume` or `--profile`), never to a subcommand (`capabilities`, `configure`, `fleet`, `smoke`), which runs as the scenario writes it; (3) and (4) as the issue says.

Rulings that go with the table:

1. A source row that needs a state the machine lacks (no catalogue entry `deepwiki`, no provider row that is not `ready`, no connected platform, no waiting pairing, no cron job free route) is `BLOCKED (environment)` with the missing state named. A candidate with such a row is not accepted; the operator fixes the state and the row is rerun (section 8.1 item 2).
2. A row's result follows its Then lines, so an output the Then requires is that row's PASS (P4-04 UAT-3: the second test of the unreachable server fails to connect, "as expected"; P4-05 UAT-3 names a T16 result as FAIL and a stop).
3. Attempts: where a source scenario allows three attempts, only the last attempt's result is the run's, and the evidence lists each (`Run 1 of 1`, with the attempts inside the block).
4. P4-04 UAT-5 ends with the operator deleting `MCP_TALARIA_KEYED_API_KEY` and `TALARIA_PROBE_VALUE` from `fixture`'s key file. The operator logs the read-back in the comment: `grep -c -e MCP_TALARIA_KEYED_API_KEY -e TALARIA_PROBE_VALUE "$FH/.env"` prints 0 (a count only).
5. **P4-07 UAT-6 conflicts with this unit's safety rule, and is reported before the freeze.** The scenario starts Talaria with `HERMES_HOME="$H" "$T"` on an empty folder, without `--no-ensure`, and expects the memory screen to say it is not connected. Without `--no-ensure` that launch runs `hermes gateway ensure`, which would start a gateway and fill `$H`; with `--no-ensure` (REQ-11 change 2) the binary prints `talaria: no Hermes gateway is running for this home` and exits 1 (section 2.2), so the screen is never reached. The row therefore cannot pass as written. The lead reports it on [`P4-07`](https://github.com/infiquetra/talaria/issues/258) in stage 0; the owner either rewrites the scenario to a form the binary supports or states in the issue that the screen cannot be reached without a gateway and replaces the scenario with a test. Until the issue says so, the row is `BLOCKED (not built)` and no candidate is accepted with it. The row is never run without `--no-ensure`.
6. P4-11 UAT-1 presses `t` (the platform test). Its test of an unset `SIGNAL_HTTP_URL` sends no request to a platform (the scenario expects `not set` and a line starting `Test:`); the smoke step omits `t` (section 9.3) and the scenario row keeps it.

### 7.3 Cross-feature rows XF-1 to XF-11

Each row: a persona, what seam it covers, steps, a binary pass rule, evidence. Shared set-up for the agent rows XF-1 to XF-8: a new `mktemp -d` folder, a Herdr pane `$PANE` made by `pane_new` and a first read check: `submit "$PANE" 'echo READ-CHECK'`, then `vis "$PANE"` or `rec "$PANE"` (whichever shows it) must hold `READ-CHECK` before any Talaria read counts. Talaria is started in a pane with `$T $F` (section 3), then the idle baseline of section 3 is taken. `$X` is the row's scratch folder. Every read and output goes through `plain` before it is checked.

**XF-1: the palette lists the seven commands, and Hermes's own `/memory` stays apart from `/memory-settings`.** Persona developer. Covers P4-04, P4-05, P4-07, P4-08, P4-11 and P4-09 with P2-03 (the palette).
- Steps: start Talaria. For each of `/mcp`, `/capab`, `/memory-s`, `/cron`, `/messa`, `/connec`, `/configure`: send the text without Enter, read, then `ctrl+c` (clears the draft) and read once to confirm an empty composer. Then send `/conf` without Enter, read, `ctrl+c`. Then `submit "$PANE" '/memory-settings'`, once the title shows read V1, `esc`; `submit "$PANE" '/memory pending'`, wait 10 s, read V2.
- Pass: the seven reads hold `/mcp`, `/capabilities`, `/memory-settings`, `/cron`, `/messaging`, `/connectors`, `/configure` respectively; the `/conf` read holds both `/config` and `/configure`; V1 holds `Memory · profile fixture`; V2 holds `No pending memory writes.` or `Pending memory writes (` and no `Memory · profile`. After the last read the composer is visible and no turn started.
- Evidence: the matching lines of the ten reads.

**XF-2: each new screen names its own profile in two panes.** Persona configurer. Covers the profile scope shared by every overlay (a screen must never mix profiles). Needs the `default` profile served (operator precondition).
- Steps: two panes `PA` and `PB`. In `PA`: `$T $F`. In `PB`: `$T --no-ensure --profile default`. In each, in turn: `/mcp` (read, `esc`), `/capabilities` (read, `esc`), `/memory-settings` (read, `esc`), `/cron` (read, `esc`), `/messaging` (read, `esc`), `/connectors` (read the consent text, `esc`; never `enter`).
- Pass: `PA` titles hold `profile fixture` for the first five screens and the consent text names `fixture`; `PB` titles hold `profile default` and its consent text names `default`; no `PA` read holds `profile default` and no `PB` read holds `profile fixture` (counted with `grep -c`, 0 each). `/configure` is left out on purpose: it starts a model session.
- Evidence: the title lines of the twelve reads and the four zero counts.

**XF-3: a secret typed anywhere in phase 4b reaches no screen, history file or recording (negative).** Persona developer. Covers P4-09 (request refusal), P4-11 (a messaging credential editor) and the recorder. Two sentinels, neither a real secret, are the only secret-shaped texts allowed (REQ-17): `S1=sk-or-v1-talaria-sentinel-0000000000000000` and `S2=uat-sentinel-5e2a`. No `ctrl+s` is sent in this row, so nothing can be saved.
- Steps: `snap > "$X/before.sum"`. Start `$T $F --record "$X/xf3.rec" 2> "$X/xf3.err"`. `submit "$PANE" "/configure use my openrouter key $S1"`, read V1; `up`, read V2; `ctrl+c`. `submit "$PANE" '/messaging'`, type `/signal`, `enter` (the card), press `down` until the selected row is `SIGNAL_HTTP_URL` (the tester quotes the read), `enter` (the editor), send `S2` by `send-text`, read V3; `esc`, read V4 (if a question about unsaved changes appears, answer with the key the screen names for discarding, quote the question, never a key that saves); `esc` until the title is gone (at most 4); `submit "$PANE" '/quit'`. In the shell: `grep -c -e talaria-sentinel -e uat-sentinel ~/.talaria/history.jsonl` (a missing file counts as 0); `grep -c -e talaria-sentinel -e uat-sentinel "$X/xf3.rec" "$X/xf3.err"`; `cat "$FH/config.yaml" "$FH/.env" 2>/dev/null | grep -c -e talaria-sentinel -e uat-sentinel` (a count only); the Talaria pane's `rec` read searched for `sentinel`; `snap > "$X/after.sum"; cmp "$X/before.sum" "$X/after.sum"; echo "cmp=$?"`.
- Pass: V1 holds `That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.` and no overlay title; V2, V3 and V4 hold no `sentinel`; every count above prints 0 (the two files of the recording count 0 each); `cmp=0`; the `rec` read holds `sentinel` only on the lines that echo the typed `/configure` command, not in any later screen.
- Evidence: V1 and the counts only.

**XF-4: with no gateway the new command-line tools fail plainly and start nothing (negative).** Persona scripter. Covers P4-05 UAT-6 and P4-09 UAT-5's command with P1-12 UAT-5 on one empty Hermes home.
- Steps: `H=$(mktemp -d)`. `HERMES_HOME="$H" "$T" capabilities skills > "$X/a.txt" 2>&1; echo "exit=$?"`; `HERMES_HOME="$H" "$T" capabilities toolsets --json > "$X/b.txt" 2>&1; echo "exit=$?"`; `HERMES_HOME="$H" "$T" configure --dry-run --json "turn on compact display" > "$X/c.txt" 2>&1; echo "exit=$?"`; `HERMES_HOME="$H" "$T" --no-ensure > "$X/d.txt" 2>&1; echo "exit=$?"` (wait at most 30 s); `ls -A "$H" | wc -l`.
- Pass: `a.txt` says no Hermes gateway is running for this home with a `Next step:` line and `exit=1` (P4-05 UAT-6); `b.txt` and `c.txt` each print a line that names the gateway (`grep -ci gateway` is 1 or more), exit non-zero, and hold no `panic` or `RUST_BACKTRACE` and no JSON proposal (`"kind":"proposal"` count 0); `d.txt` holds `talaria: no Hermes gateway is running for this home` and `exit=1`; the folder listing count is 0. The exact text of `b.txt` and `c.txt` is each owner's; the tester quotes it and a text that does not mention the gateway is a FAIL reported to the owner.
- Evidence: the four outputs (scrubbed) and the count.

**XF-5: opening every new screen sends nothing it should not (negative).** Persona developer. Covers the consent rule of P4-11, the no-`unattended` rule and the no-write rule on all six overlays, read from the recorder.
- Steps: `snap > "$X/before.sum"; cronids > "$X/cron-before.txt"`. Start `$T $F --record "$X/xf5.rec" 2> "$X/xf5.err"`. In turn `submit` `/mcp` (read, `esc`), `/capabilities` (read, `esc`), `/memory-settings` (read, `esc`), `/cron` (read, `esc`), `/messaging` (read, `esc`), `/connectors` (read the consent text, `esc`; never `enter`); then `/quit`. Then `grep -c . "$X/xf5.rec"`; `grep -c 'unattended' "$X/xf5.rec"`; `grep -c '"connectors\.' "$X/xf5.rec"`; `grep -c 'reload\.mcp' "$X/xf5.rec"`; `grep -c 'config\.set' "$X/xf5.rec"`; `snap > "$X/after.sum"; cmp "$X/before.sum" "$X/after.sum"; echo "cmp=$?"`; `cronids` compared before and after (`croncmp=`).
- Pass: the first count (the recording is not empty) is 1 or more; the next four print 0; `cmp=0` and `croncmp=0`; each read held its overlay's title or consent text (the XF-2 texts).
- Evidence: the five counts and the `cmp=` and `croncmp=` lines.

**XF-6: screens opened during a running turn leave the turn running.** Persona developer. Covers the six read overlays with P1-08 (Esc) and P2-03 UAT-7 (commands while busy). `/configure` is left out: it starts a model session of its own (section 10.4 item 7).
- Steps: start Talaria; `submit "$PANE" 'Count from 1 to 2000, one number per line, and do not stop early.'`; wait for 15 counting lines (a number alone on its line, value 1 to 2000, taken after the last echo of the prompt). Then for each of `/mcp`, `/capabilities`, `/memory-settings`, `/cron`, `/messaging`, `/connectors`: `submit`, wait 2 s, read; if the screen's title (or the consent text) is showing, `esc`, wait 400 ms, read. Never send `esc` when no overlay is showing (it would interrupt the turn). Finally read the highest counting value twice, 1 s apart (M0, M1).
- Pass: every attempt's first read shows either the screen's title or a line containing `can't run mid-turn` that was not in the read before the attempt (the busy refusal P2-03 UAT-7 names); anything else, including an unchanged screen, is a FAIL. After the last attempt the counting is still growing (M1 above M0) or a counting line with the value 2000 is visible; it never restarts from 1 twice; the composer is visible. If the turn ends by itself before the six attempts are done, repeat the row once with 5000 in place of 2000 (counts toward the three runs).
- Evidence: the reads' matching lines and M0, M1.

**XF-7: all six screens fit 80 by 24.** Persona developer. Covers the new overlays (the screens' 80 by 24 rule is the architecture's). Needs `tmux`.
- Steps: `tmux new-session -d -s accxf7 -x 80 -y 24 -c "$X" "unset HERDR_ENV HERDR_PANE_ID HERDR_BIN_PATH; $T $F"`. Take this session's own idle baseline (a Herdr pane's baseline is another size and never used here): every 2 s for at most 30 s, `tmux capture-pane -p -J -t accxf7 | plain > "$X/xf7-idle-<k>.txt"`, until two captures in a row are identical and the last three non-blank lines contain no overlay title; keep those three lines as `$X/xf7-idle-tail.txt`. No stable capture within 30 s: the row FAILs with `the 80 by 24 screen never settled` and the last capture. For each of `/mcp`, `/capabilities`, `/memory-settings`, `/cron`, `/messaging`, `/connectors` (n = 1 to 6): `tmux send-keys -t accxf7 -l '<command>'`; `tmux send-keys -t accxf7 Enter`; `sleep 3`; `tmux capture-pane -p -J -t accxf7 > "$X/xf7-<n>.txt"`; `tmux send-keys -t accxf7 Escape`; `sleep 1`; `tmux capture-pane -p -J -t accxf7 > "$X/xf7-<n>-closed.txt"`; if the screen's title is still in it, send Escape once more, wait 1 s and capture again. `tmux kill-session -t accxf7`.
- Pass: for each open capture: it holds the screen's title text (`MCP servers · profile fixture`, `Capabilities · profile fixture`, `Memory · profile fixture`, `Scheduled jobs · profile fixture`, `Connectors and messaging · profile fixture`; for `/connectors` the consent text `enter continue · esc back`), so the top is not clipped; the display-width check `python3 -c 'import sys,unicodedata as u; print(sum(1 for l in sys.stdin.read().split("\n") if sum(2 if u.east_asian_width(c) in "WF" else 0 if u.combining(c) else 1 for c in l.rstrip()) > 80))' < file` prints 0; the last non-blank line is the screen's key hint line, so the bottom is not clipped: it equals the last non-blank line of the same screen's recording under `tools/smoke/tests/fixtures/screens/phase4b/`, or, where the hint is shortened to fit 80 columns, is a non-empty prefix of it that holds `Esc` (the tester quotes both). For each closed capture: no title text of any of the six screens, and its last three non-blank lines (after `plain`) equal `$X/xf7-idle-tail.txt` (the composer rule of section 3, against this session's baseline).
- Evidence: the six title lines, the six counts, the six quoted hint lines and the six closed-capture checks.

**XF-8: idle cost after every new screen was used.** Persona developer. Covers the six screens, the idle budget of P1-11 and the zero-idle-bytes rule of the overlays (P4-04 AC-7, P4-11 AC-10).
- Steps: a new folder; start Talaria in `$PANE` with the release binary. After 5 s at an empty composer: `PID=$(herdr pane process-info --pane "$PANE" | jq -r '.result.process_info.foreground_processes[] | select(.cmdline | contains("target/release/talaria")) | .pid')`; sample 0: `R0=$(ps -o rss= -p "$PID")`. Then open and close each screen once (`/mcp`, `/capabilities`, `/memory-settings`, `/cron`, `/messaging`, `/connectors`: `submit`, wait until its title or consent text shows, `esc`). Wait 5 s. Sample 1: `R1`, `C1` (`ps -o time= -p "$PID"`), `V1=$(vis "$PANE")`. `sleep 120`. Sample 2: `R2`, `C2`, `V2`. Convert `time=` with `awk -F: '{ s = 0; for (i = 1; i <= NF; i++) s = s * 60 + $i; print s }'`.
- Pass: `R0` at most 40960 (KiB, the 40 mebibyte start budget); `R2` at most 102400 (the 100 mebibyte ceiling); `R2 - R1` at most 4096; `C2 - C1` at most 1 second; `V1` and `V2` identical (`cmp` of the two `plain` reads). The operator precondition is that no status-line command is configured. A line that still changes by itself is quoted, the row is a FAIL and it is reported on P1-10. The first two limits are the architecture's budgets ("MB" read as mebibytes, as P1-11 decision 7 does); the growth and CPU limits are this issue's own sanity checks, the same as the earlier phases'. A `ps` or `jq` that does not work on the machine makes the row `BLOCKED (environment)`.
- Evidence: the three samples and conversions, the `cmp` result.

**XF-9: one cron job, two views: `/configure` pauses the job that `/cron` created (operator).** Persona configurer. Covers P4-09 with P4-08 (`/configure` applies through the cron service). Runs in the sitting (section 7.5).
- Steps: in `$T $F`, create the job as P4-08 UAT-2 does: Ctrl+N, Name `talaria-uat-xf`, Tab, Schedule `0 3 1 1 *`, Tab, Prompt `Reply with the single word ok.`, Ctrl+S; read; back to the composer (section 3). Then `/configure pause the scheduled job talaria-uat-xf`, read the review (V1); `a`, `y`, read (V2), Enter; back to the composer; `/cron`, read (V3); select the row, Ctrl+E (resume), read (V4); Ctrl+D, `y` (remove), read (V5); Esc.
- Pass: V1 lists exactly one row that names `talaria-uat-xf` and the word `pause`, selected, and no other change; V2 reads `Configured profile fixture: 1 applied, 0 failed, 0 unknown, 0 not attempted.`; V3 shows the row `paused`; V4 shows `next in`; V5 shows no `talaria-uat-xf` row and `Removed talaria-uat-xf.`
- Evidence: the five reads. The wording of the proposal row is P4-09's and P4-08's: the operator quotes it; a review that proposes a different job, or more than one change, is a FAIL.

**XF-10: `/configure` turns a toolset off and `/capabilities` shows it (operator).** Persona configurer. Covers P4-09 with P4-05 (`/configure` applies through the capabilities service).
- Steps: before the row the operator notes one enabled toolset of `fixture` (`/capabilities`, `3`, read), then goes back to the composer (section 3: the Toolsets tab is closed before anything is typed). Then `/configure turn off the <toolset> toolset`, read the review (V1); `a`, `y`, read (V2), Enter; back to the composer; `/capabilities`, `3`, read (V3); Space on the toolset to turn it on again, read (V4); back to the composer.
- Pass: V1 lists exactly one row naming the toolset and the word `off` (or `disable`: the operator quotes P4-09's wording), selected; V2 reads `Configured profile fixture: 1 applied, 0 failed, 0 unknown, 0 not attempted.`; V3 shows the toolset `off`; V4 shows it `on`, the starting state.
- Evidence: the four reads and the read-back of the toolset's state.

**XF-11: an MCP server installed in one pane is adopted and used by another open chat (operator).** Persona configurer. Covers P4-04's reload gate across two chats of one profile. Hermes refreshes only the calling chat when MCP is applied; another chat that is already open takes the change when it applies for itself or is reopened (section 2.2). This row tests that supported path: the second chat's own `/mcp apply`.
- Steps: two panes with `$T $F`: `PA` and `PB`, both at an idle composer; `PB` stays idle until its own step. In `PA`: `/mcp`, Tab, type `deepwiki`, Enter, Enter, wait for `· test ok, <n> tools`; Ctrl+Y, read the confirmation (R1), `y`, read (R2); back to the composer (section 3). In `PB`: submit `/mcp apply`, read the confirmation (R3), `y`, read (R4); back to the composer. In `PB`: send `Use the deepwiki tools to say in one sentence what the GitHub repository NousResearch/hermes-agent is.` (up to three attempts). Cleanup, in `PA`: `/mcp` (the list opens unfiltered: the earlier overlay was closed), select `deepwiki`, Ctrl+D, `y`, Ctrl+Y, `y`, read (R5); back to the composer. In `PB`: `/mcp apply`, `y`, back to the composer.
- Pass: R1 and R3 show `Reloading MCP refreshes every chat of this profile.`; R2 starts `MCP servers reloaded.`; R4 reads `MCP servers reloaded. This session's tools change from its next turn.` (Hermes's `session: applied`; the `unchanged` text there means `PB` did not adopt the server, a FAIL); `PB`'s transcript shows a call to a deepwiki tool within three attempts (three misses: FAIL with the transcripts; one rerun allowed); R5 has no `deepwiki` row.
- Evidence: R1 to R5, `PB`'s transcript with the call line, the attempts.

### 7.4 The operator's own check: OPC-1 to OPC-5

This is the roadmap's phase 4 operator check, made concrete: "add a provider key, add an MCP server, change a profile's model and create a cron job without leaving Talaria or opening a YAML file". The operator runs it in one sitting at the start (section 7.5), in one Talaria session as far as the steps allow, on the `fixture` profile, with the candidate binary `$T`. Each row has its own record line, evidence section and comment. Sources: P4-02 UAT-3 (a key), P4-04 UAT-2 (an MCP server), P4-03 UAT-2 (the main model), P4-08 UAT-2 and UAT-3 (a cron job); the steps are theirs, joined.

Common preconditions (the operator sets them up; agents never do): the operator precondition list of the issue; `fixture` has no OpenRouter key (the phase 4a precondition), and the operator holds an OpenRouter key of their own (any provider `fixture` has no key for works; the rows say OpenRouter); `deepwiki` is not installed in `fixture`; `fixture`'s cron model is a free one (the free route of P4-08 UAT-3); the main model is noted (`minimax-m3.1-flash` on `ollama-cloud`); the sitting's digest snapshot is taken (section 7.5). The operator works in two panes: `PS`, a shell pane for `snap` and `cmp` only, and `P1`, where Talaria runs. The prohibition on outside tools covers items 1 to 5 (the OPC story) only: during them the operator opens no editor and no configuration file and runs no `hermes` command; OPC-5 proves it. Later items may use exactly the outside commands their source scenarios prescribe (for example the key-file cleanup of P4-04 UAT-5 and the uninstall command of P4-05 UAT-3) and the restore steps of section 7.5.

**The history boundary.** In `PS` and in the shell pane that starts `P1`, after the common settings are loaded and immediately before OPC-1, the operator runs the marker command `: OPC-START` (a no-op that only lands in the shell's history). OPC-5's audit counts only history entries after the last marker and skips its own entry, so setup commands (for example the `. ../acc.env` that loads the settings) and the audit's own text never count.

**OPC-1: add a provider key.** Persona configurer.
- Steps: in `P1`: `"$T" $F`. `/providers`; the Keys tab is open; type `openrouter`, Enter (the card `API key for OpenRouter`), paste the operator's own key. Read the card now, with the key pasted and **before** the Enter that validates and saves it (R1). Then press Enter and read again once the card has closed (R2).
- Pass: R1 shows the open card with 40 `•` then `…` (or one bullet per character for a shorter key), and no part of the key beyond the screen's own mask; within 5 s of the Enter the card closes, `Saved the key for OpenRouter.` appears and the OpenRouter row reads `ready ·` then a model count and `key …` with the last four characters (P4-02 UAT-3's text for a saved key); `snap` taken in `PS` right after differs from the sitting's first snapshot (`cmp=1`: the control of section 7.5). `grep -c` of the key's first eight characters over the `rec` read of `P1` prints 0.
- Evidence: R1, R2, the `cmp=` line, the count; never the key.

**OPC-2: add an MCP server.** Persona configurer.
- Steps: in `P1`, back to the composer (section 3): the providers list still holds the filter `openrouter`, so one Esc clears the filter and a second closes the overlay; read the idle composer. `/mcp`, `tab`, type `deepwiki`, Enter, Enter, wait; read (R1). Then Ctrl+Y, read the confirmation (R2), `y`, read (R3); back to the composer.
- Pass: R1's `deepwiki` row ends `· test ok, <n> tools` with n above 0 and the notice line starts `deepwiki works: <n> tools`; R2 shows `Reloading MCP refreshes every chat of this profile.`; R3 starts `MCP servers reloaded.`
- Evidence: the three reads.

**OPC-3: change the profile's main model and use it, with the new server.** Persona configurer.
- Steps: in `P1`, from the idle composer: `/models`; on Main press Enter, type `openrouter`, Enter, choose a model the operator accepts to use (a free one if the account has it), Ctrl+S, read the diff (R1), Enter (and `y` if a cost confirmation appears), read (R2); back to the composer. Then `/quit` and start a new session without the model flag (a new session attaches after OPC-2's apply, so it has the server; section 2.2): `"$T" --no-ensure --profile fixture`; read the status line (R3). Send `Use the deepwiki tools to say in one sentence what the GitHub repository NousResearch/hermes-agent is.` (up to three attempts), read (R4). `/quit`.
- Pass: R1 shows `model.provider` and `model.default` before and after; R2 shows the new model and `Applies to new sessions.`; R3's status line shows the new model; R4's transcript shows a call to a deepwiki tool and an answer, within three attempts (three misses: FAIL with transcripts; one rerun allowed).
- Evidence: R1 to R4 and the attempts.

**OPC-4: create a cron job and run it.** Persona configurer.
- Steps: in `P1` (`"$T" $F` again): `/cron`; Ctrl+N; Name `talaria-opc-job`, Tab, Schedule `0 3 1 1 *`, Tab, Prompt `Reply with the single word ok.`, Ctrl+S; read (R1). Select the row, Ctrl+T, read the confirmation (R2), `y`, wait for the notice (at most 5 minutes), read (R3); Enter, read (R4); back to the composer.
- Pass: R1 shows a notice starting `Cron job 'talaria-opc-job' created.` and the row `next in <days>d…`; R2 names the job, `fixture`, `this profile's cron model` and `local`; after `y` the row reads `running now (started here)`; R3's notice reads `Ran talaria-opc-job: finished ok.`; R4's `Recent runs` has a `finished` row with non-zero tokens.
- Evidence: R1 to R4, the time from `y` to the notice.

**OPC-5: nothing was done outside Talaria, and every value is put back.** Persona configurer.
- Steps, part A (no outside tools), the first commands of OPC-5, before any other command of this row: in `PS` and in the shell pane that started `P1`, run the audit (bash; in zsh write `history 1` in place of `history`, because zsh's bare `history` lists only the last 16 entries):

  ```bash
  history | awk '/: OPC-START$/ { m = 1; n = 0; next } /OPC-AUDIT/ { next } m && /(^|[ \/;|&(])(vi|vim|nvim|nano|emacs|code|hx|micro|hermes)( |$)|\.ya?ml|\.env/ { n++ } END { t = "OPC-AUDIT"; print m ? n : "no marker" }'
  ```

  It counts matching entries after the last `: OPC-START` marker only, and skips every entry holding the text `OPC-AUDIT`, which is its own (the token sits inside the program so the line excludes itself). Checked while drafting in interactive bash and zsh: clean history prints `0`, a `cat x.yaml` after the marker prints `1`, a history without the marker prints `no marker`; the earlier form `history | tail -n 80 | grep -cE '...\.env'` printed `1` on a clean history because it matched its own entry. The operator writes one sentence in the comment: they opened no editor or configuration file and ran no `hermes` command during OPC-1 to OPC-4. Part B (the writes were real): `snap > "$X/mid.sum"; cmp "$X/sitting-before.sum" "$X/mid.sum"; echo "cmp=$?"` right after OPC-1. Part C (restore, still in Talaria): `/cron`, select `talaria-opc-job`, Ctrl+D, `y`; `/mcp`, select `deepwiki`, Ctrl+D, `y`, Ctrl+Y, `y`; `/models`, set Main back to the noted model and provider, Ctrl+S, Enter; `/providers`, select OpenRouter, Ctrl+D, `y`. Read each result, then read each value back: the cron list, the MCP list, `/models` Main, the OpenRouter row.
- Pass: part A prints `0` in both shells (`no marker` or any other number is a FAIL) and the sentence is in the comment; part B printed `cmp=1`; part C reads: no `talaria-opc-job` row and `Removed talaria-opc-job.`, `cronids` prints the same ids as before the sitting, no `deepwiki` row, Main equals the noted model and provider, the OpenRouter row reads `no key`; `grep -c` of the key's first eight characters over the `rec` read of `P1` and over `"$FH/config.yaml"` prints 0.
- Evidence: the count, the `cmp=` lines, the four read-backs, the operator's sentence, the comment URL.

### 7.5 The operator sitting (30 items) and OP-1

The operator runs these with the candidate binary `$T`, in this order, and posts one comment per item on this issue, the PH4-ACC issue (`<id>: PASS`, `FAIL` or `BLOCKED (<cause>)`, with counts and screenshots). Writes come before anything long-running or that touches an account. "One sitting" means: the 30 items in this order, on one candidate, with no agent live row run between item 1 and item 30. The operator may pause between items (even over days); after a pause they read `fleet` and the doctor report before the next item, and a pause never splits an item or the OPC story (items 1 to 5).

| # | Row | Writes? |
|---|---|---|
| 1 | OPC-1 (a provider key) | yes: the key file |
| 2 | OPC-2 (an MCP server) | yes: the profile's MCP list |
| 3 | OPC-3 (the main model, used with the server) | yes: `model.provider`, `model.default` |
| 4 | OPC-4 (a cron job, run once) | yes: a job and one run |
| 5 | OPC-5 (outside tools, restore, read-back) | yes: removes what items 1 to 4 added |
| 6 to 10 | P4-04 UAT-2, UAT-3, UAT-5, UAT-4, UAT-6 | an MCP server added and removed; a key file cleaned; an OAuth sign-in |
| 11 to 13 | P4-05 UAT-2, UAT-3, UAT-4 | a skill and a toolset toggled; a skill and a plugin installed and removed |
| 14, 15 | P4-07 UAT-4, UAT-5 | the curator paused and run; `USER.md` emptied |
| 16 to 20 | P4-08 UAT-2, UAT-3, UAT-4, UAT-5, UAT-6 | cron jobs created and removed; one run |
| 21 to 24 | P4-11 UAT-3, UAT-4, UAT-5, UAT-6 | a messaging credential; connector accounts; a pairing |
| 25, 26 | P4-09 UAT-1, UAT-2 | `display.bell_on_complete` set and restored; an MCP server added and removed |
| 27, 28 | XF-9, XF-10 | a cron job paused; a toolset toggled and restored |
| 29 | XF-11 | an MCP server added and removed, used from a second pane |
| 30 | OP-1 (the live write suites, below) | yes, restored by each test |

The sitting's own order within a group is the order in the table (P4-04 UAT-5 before UAT-4 so the key file is cleaned before the sign-in). The operator may drop an item only by section 8.1 item 4.

**The digest control.** Before item 1 the operator runs `snap > "$X/sitting-before.sum"; cronids > "$X/sitting-cron.txt"` in `PS`, then the marker `: OPC-START` (section 7.4). Right after OPC-1 saves the key, `snap > "$X/mid.sum"; cmp "$X/sitting-before.sum" "$X/mid.sum"; echo "cmp=$?"` must print `cmp=1`: this proves the snapshot used by the agents' read-only checks sees a one-file change. A `cmp=0` here makes item 1 FAIL and every agent `cmp=0` result is then unproven (report it on the issue and stop). After the sitting the operator runs `snap` again and records `cmp` against the first snapshot as information only: Hermes may rewrite a file's layout or comments when it saves, so a restored value with different bytes is not a FAIL; the value read-back is the restoration proof. Cron changes never move `snap` (section 2.2); whether MCP and skill changes do is section 10.4 item 3.

**OP-1: the live write suites.** The operator exports the opt-in variables only in the shell that runs these commands, one at a time, each with `TALARIA_LIVE_REQUIRE=1`:

```bash
TALARIA_PROBE_WRITES=1 cargo test -p talaria-cli --test live_mcp -- --ignored --nocapture --test-threads=1 live_add_test_toggle_remove_on_fixture
TALARIA_PROBE_WRITES=1 cargo test -p talaria-cli --test live_capabilities -- --ignored --nocapture live_capabilities_write_on_fixture
TALARIA_PROBE_WRITES=1 cargo test -p talaria-app --test live_memory pause_round_trip_on_fixture -- --ignored --nocapture
TALARIA_PROBE_WRITES=1 TALARIA_CRON_FREE_ROUTE=1 cargo test -p talaria-cli --test live_cron -- --ignored --nocapture --test-threads=1 \
  live_add_pause_resume_remove_on_fixture live_run_now_and_history_on_fixture
TALARIA_PROBE_CONNECTORS=1 cargo test -p talaria-cli --test live_connectors -- --ignored --nocapture live_connectors_gate_on_fixture
TALARIA_PROBE_WRITES=1 cargo test -p talaria-cli --test live_connectors -- --ignored --nocapture live_messaging_write_round_trip
TALARIA_PROBE_WRITES=1 cargo test -p talaria-cli --test live_configure -- --ignored --nocapture live_apply_round_trip_on_fixture
```

PASS: each exits 0 and prints its signal: `LIVE_MCP_RESULT: PASSED`; `LIVE_CAPABILITIES_WRITE: PASSED`; `LIVE_MEMORY_WRITE_RESULT: PASSED`; `LIVE_CRON_RESULT: PASSED` twice and one `LIVE_CRON_CHANGED:` line; `LIVE_CONNECTORS_RESULT: PASSED` for each of the two connectors commands; `LIVE_CONFIGURE_RESULT: PASSED`; and none prints `SKIPPED`. Each test restores and reads back its own values. The operator unsets the variables afterwards. Evidence: the signal lines and the operator's note.

Operator comments follow the phase 2a format: one comment per item; when all 30 pass, one comment on this issue with the line `PHASE 4 OPERATOR ROWS: PASSED (candidate <7 hex>)` (AC-8 reads this issue's comments, never the parent's). An agent copies each comment into the evidence with its URL. The operator restores and reads back every value they changed (the OpenRouter key row, the main model and provider, `deepwiki`, the cron jobs, a skill and a toolset, a plugin, the curator state, `USER.md` where it was filled, `display.bell_on_complete`, the messaging field, any connector account and pairing) and logs the read-back in the comment of the item that changed it. Two values need a copy taken before the item, because the item destroys them: before item 15 (P4-07 UAT-5) the operator copies `"$FH/USER.md"` to a private folder outside the repository and after the item copies it back and logs `cmp` of the two (`cmp=0`); before item 14 (P4-07 UAT-4, the curator run) the operator notes the skill list (`/capabilities`, Skills tab) and after it restores any skill the curator archived with `hermes -p fixture curator restore <skill>` (the `restore` subcommand of `hermes_cli/curator.py`, line 660 at Hermes v0.22.4), then logs the skill list read-back. Neither copy nor list is pasted into the evidence; only the `cmp=` line and the names of restored skills are.

## 8. Run order, failures and a new candidate

1. Stage 0: build the smoke extension and merge it (section 9). Report P4-07 UAT-6 (section 7.2 ruling 5). Freeze (section 3).
2. Stage 1 (agent tester, one pane at the worktree root): AUTO-1 to AUTO-7, then AUTO-8 to AUTO-14 back to back.
3. Stage 2 (agent tester): the 15 source rows the operator does not run, in issue order (section 7.2), then XF-1 to XF-8 in order, one live scenario at a time.
4. Stage 3 (operator): section 7.5 (30 items).
5. Stage 4: write the record and evidence (the first commit is the empty record, section 4), run the checker, open the pull request (documentation only), have it reviewed, merge. The pull request body carries the lines of the issue's REQ-16.

The issue's own six scenarios fall in these stages: UAT-3 is AUTO-13 and AUTO-14 themselves (the same two runs, outputs `$X/s6.txt` and `$X/s7.txt`, evidence in those rows' sections; it is not run a third time); UAT-4 runs in stage 2, after XF-8; UAT-5 and UAT-6 are stage 3 (UAT-5 is items 1 to 5, UAT-6 items 6 to 30); UAT-1 and UAT-2 run in stage 4, on the record branch, and their results go in the pull request body (REQ-16).

Three identical provider errors in a row (same HTTP status, same message) stop the run: report them on the issue and ask the operator, never retry in a loop. A row may be run up to three times on one candidate; every run goes in its evidence section with why each earlier failure was not a Talaria defect (a provider rate limit, a model that did not follow a plain prompt).

A FAIL stays FAIL until a new candidate passes it. The tester comments the pane text on the issue; the lead files a defect issue through Mission Control for the owner of the source issue (the operator files those found in their rows, or asks the lead to), naming the source issue and never making it a sub-issue of the parent, whose membership check counts exactly its seven children; the fix merges; the lead freezes a new candidate (section 3) and every row is run again; the record is overwritten (git history keeps the old one). A defect is never fixed inside this issue, and no scenario's expected result is edited to make it pass; if a source issue's expected text is wrong, its owner corrects the source issue and this unit's Appendix A is corrected (a correction of the specification is the one change allowed after the freeze).

### 8.1 Rulings that settle questions a builder asked

These settle points where two rules of the issue could be read against each other. They change no pass rule; every one keeps "only PASS is a pass". They are the rulings of section 8.1 of [PH4A-ACC](PH4A-ACC.md), restated with this unit's numbers.

1. **A row's label follows its cause, not the exit code.** For a skip the row does not expect: a suite run that prints `PASSED WITH SKIPS` (exit 3) is BLOCKED (environment). Under `TALARIA_LIVE_REQUIRE=1` a skipped step prints `FAIL <step> (skipped: <reason>; TALARIA_LIVE_REQUIRE=1)` and the run exits 1; the line says `(skipped:`, so the cause is the environment and the row is `BLOCKED (environment: <step> skipped)`, not FAIL. Any other `FAIL <step> (...)` line, including `deadline exceeded in step <name>`, is a FAIL of the row unless item 2 shows it was not a Talaria defect. The checker accepts only the four forms and `BLOCKED (<cause>)` takes any cause text without a closing bracket. This ruling applies to the five earlier suites (AUTO-8 to AUTO-12), which have skip paths. The phase 4b suite has none: a phase4b `SKIP` line or `PASSED WITH SKIPS` is a defect of the suite and a FAIL.
2. **Reruns on the same candidate.** A row may be run up to three times on one candidate. A BLOCKED row is rerun once the cause is fixed (the comment names who fixed what). A FAIL may be rerun only when the tester shows it was not a Talaria defect: the pane text shows a provider error (status and message) or the model not following a plain prompt, while every wording and key of Talaria was as its source issue says; and the lead writes `rerun approved: <row id>: <cause>` on the issue before the rerun. A FAIL that is a Talaria defect (a wording that differs, a key that does nothing, a crash, a wrong state, a write that should not have happened) is final for the candidate: the row is rerun only on a new candidate. When it is unclear which it is, treat it as a defect, because another candidate costs less than a wrong pass. The record holds the result of the last run; the evidence lists every run.
3. **AUTO-1 when the dispatch run reports nothing on the candidate.** BLOCKED (environment), reported on P1-11. No candidate is accepted with it BLOCKED. Because the answer can be had in stage 0, the operator then has P1-11's workflow fixed before the freeze, or edits AUTO-1 to drop its bench-run part (settled decision 2).
4. **Dropping a row or a step before the freeze (settled decision 2).** Let N be 67 minus the rows dropped, and S 14 minus the smoke steps dropped. In one commit whose message gives the reason, the lead changes: in the issue, every `67` (Objective, user-visible result, REQ-7, REQ-9, the Tests paragraph, UAT-1, UAT-2, AC-5, AC-9), the row counts `14 + 36 + 11 + 5 + 1` (AC-7), `36`, `11`, the 21 operator source rows, the 15 agent source rows and the 30 operator items (REQ-11 to REQ-14, UAT-5, UAT-6, AC-8) as far as they drop, and, for a dropped step, every `14` (REQ-2, REQ-10, UAT-3, the AUTO-13 and AUTO-14 pass rule); here, the section 4 skeleton (rows removed) and the red-run line; and in the checker, the `IDS=` line (the only list: the test script builds its rows from it). In the test script: case 1's `67 of 67` becomes `N of N`; a case that names a dropped id (`P4-09 UAT-6`, `AUTO-5`, `XF-2`, `P4-08 UAT-2`, `AUTO-1`, `OP-1`, `P4-11 UAT-6`, `XF-1`, `AUTO-2`, `AUTO-3`, `XF-6`) names another kept id in its place, with the same role; the number of cases stays 26. For a dropped smoke step, the test author deletes that step's functions, screens and wording constants in a commit `test(smoke): drop step <name> (operator decision)`, and the counts of functions and steps are updated. The lead lists every number changed in a comment on the issue before the freeze. Dropping after the freeze is not possible: it is a new candidate.
5. **A source row whose Hermes behaviour differs on the candidate's Hermes release.** The source issue says which facts to re-verify at the pinned commit. If a source issue's final text says a call moved to another transport, the source rows follow the issue as edited by the planner; a row that only passes with the old transport is a FAIL reported to the owner.
6. **A key card, a credential editor and a review are never confirmed by an agent.** An agent row or smoke step that would press Enter in a key card, `Ctrl+S` in an editor or review, or `y` on a confirmation that writes, is out of bounds, whatever the content. Such a scenario is the operator's (this is why the 21 rows of section 7.2 are the operator's).
7. **The operator's check needs the operator's account.** OPC-1 needs a provider key the operator holds; if they hold none, they say so on the parent before the freeze and the lead and operator agree a substitute provider for OPC-1 and OPC-3 (the check's point is that a key is added and used from inside Talaria), or drop the pair by item 4.

## 9. The smoke suite extension

### 9.1 Why a sixth suite, and what it is not

The phase 1 suite proves that a conversation works. The phase 4a suite proves the five configuration screens and two command-line tools. The new suite proves that the six phase 4b overlays open, show what Hermes holds, refuse bad input and close, that the two command-line tools print what their issues say, and that a secret in a `/configure` request is refused, all on a real terminal against the live gateway. It is black-box like the others: it launches the built binary through `herdr`, sends keys and reads the screen. It is not a replacement for the scenarios (it checks the first screens and the refusals, not the writes), it does not run the earlier suites, and it needs no `tmux`. Above all it writes nothing and makes no outside call itself: the only outside traffic it causes is the gateway's call to the model provider for the plain-answer turn and the token C turn after resume (ordinary conversation, as in phase 1); no step confirms a review, saves a key or a credential, signs in, tests a platform, runs a job, starts a `/configure` session, restarts or creates (REQ-5). The suite is selected with `--suite phase4b`.

### 9.2 Shape of the code (proposed names; report any change)

In `tools/smoke/src/`: `suite4b.rs` (`run_smoke_4b`, the step table), `steps4b.rs` (one function per step), `screen4b.rs` (pure screen helpers), `wording4b.rs` (the texts, 9.5). Edits to existing files are minimal and listed in the pull request: `suite.rs` (`Phase4b` joins the `Suite` enum and its `FromStr` accepts `phase4b`; read the earlier `cli*.rs` tests first: if one pins the parse text exactly, leave that text as it is, because AC-2 forbids editing that test, and say so in the pull request; `--help` names all six suites), `config.rs` (the `phase4b` deadline default 900; no second enum), `main.rs` (dispatch and the preflight), `lib.rs`.

```rust
// suite.rs (existing; one variant and one parser value added)
pub enum Suite { Phase1, Phase2a, Phase2b, Phase3, Phase4a, Phase4b }    // FromStr: "phase1" | "phase2a" | "phase2b" | "phase3" | "phase4a" | "phase4b"
// suite4b.rs
pub struct Phase4bConfig { pub empty_home_parent: std::path::PathBuf }   // where the no-gateway step makes its empty folder (default: the system temporary folder)
pub fn run_smoke_4b(term: &dyn Terminal, clock: &dyn Clock, cfg: &SmokeConfig,
                    cfg4b: &Phase4bConfig, stop: &std::sync::atomic::AtomicBool) -> SmokeReport;
```

The phase 4b preflight (in `main`, before anything is created; exit 2, one sentence, no `SMOKE_RESULT` line), in this order: (1) `jq` is not on the `PATH`: `the phase4b suite needs jq on the PATH to parse the capabilities JSON; install jq and run it again`; (2) `--profile`, `--model` or `--provider` was given with a value other than `fixture`, `minimax-m3.1-flash` or `ollama-cloud`: `the phase4b suite checks texts that name profile fixture, model minimax-m3.1-flash and provider ollama-cloud; run it without --profile, --model and --provider`. "Nothing created" means: no Herdr tab (no `tab create` call), no report folder or file, no scratch project and no empty-home folder. `cli4b.rs` observes it through `fake-herdr`'s call log and two empty test folders passed as `--report-dir` and as the temporary root, both still empty after the run. The earlier suites keep their own option handling.

`screen4b.rs` (pure functions over screen text, each with unit tests on recorded screens; the 14 names of 9.9): title finders (`overlay_title_found(text, title_fmt, profile)`), row parsers (`mcp_rows(text)`, `catalogue_rows(text)`, `capabilities_tab_rows(text)`, `cron_rows(text)`, `platform_rows(text)`), `memory_sections(text)`, `refusal_found(text, exact)`, `write_banner_found(text)`, `consent_found(text, profile)`, `idle_screen(text) -> bool` (no title of 9.5 visible and the composer shown).

### 9.3 Flow, step by step

Common to all steps: a step begins when the previous one passed; a step reads with `visible` unless the line says otherwise; a failing step ends the run with its FAIL line of 9.4 and the pane capture; every read of a screen step also runs `write_banner_found` and fails with the write-banner reason if it is true. The guard reports a write after it happened; what prevents a write is the key list below.

**The idle state and the key rule.** Steps 3 to 10 each start and end in the idle state: Talaria running in the first pane, no overlay title of 9.5 visible, an empty composer, no turn running. A screen step first reads the pane (R0) and fails with `the step did not start from an idle composer` unless `idle_screen(R0)` is true. It then sends, in order: its setup keys, the source scenario's When keys up to its last read, and the standard teardown. The standard teardown is: while the screen's title is visible, send `esc`, wait 400 ms and read, at most 3 times; if the title is still visible after the third, fail with `the <screen> overlay did not close after 3 Esc`. When the source keys already closed the overlay, the teardown sends nothing. Steps 6 and 10 open no overlay (step 5 does: `/memory-settings`, so it uses the standard teardown); steps 6 and 10 never send `esc` and end idle by themselves (step 10's teardown is `ctrl+c`, a 2.5 s wait and a read, as step 5 of phase 4a). A source scenario's `/quit`, and any key after its last read, is omitted. "Type `x`" is `herdr pane send-text` of `x` with no Enter; "submit `x`" is `send-text` of `x`, a 300 ms wait (beyond the 120 ms in which P1-13 found that Talaria reads an Enter after a paste burst as a line break), then `send-keys enter`; a 400 ms wait follows every `esc`. P1-13's one logged submit fallback is allowed for a submitted slash line or prompt: if within 5 s the composer still holds the submitted text and no title, refusal or reply has appeared, send one `enter` and print `NOTE submit fallback: sent Enter (step <name>)`. It is never sent while a card, the consent text or any overlay title is visible, and never twice for one submission; `typed_input4b.rs` allows exactly this one extra `enter` after a submission and nowhere else. Every key and text in this table, in this order, is the allowed list of `typed_input4b.rs`.

| # | Step | Setup keys | Source keys and reads | Teardown |
|---|---|---|---|---|
| 3 | mcp-browse | none | submit `/mcp`, R1 once `MCP servers · profile fixture` shows; `tab`, R2; type `deepwiki`, R3; `esc`, `esc` | standard (normally nothing left) |
| 4 | capabilities-browse | none | submit `/capabilities`, R1 once `Capabilities · profile fixture` shows; type `3`, R2; type `4`, R3; type `a`, R4; `esc`, R5 | standard |
| 5 | memory-open | none | submit `/memory-settings`, R1 once `Memory · profile fixture` shows; `esc`, R2 | standard (normally nothing left) |
| 6 | memory-commands | none | submit `/memory pending`, wait 10 s, R1; submit `/curator status`, wait 10 s, R2 | none (no overlay opens) |
| 7 | cron-browse | none | submit `/cron`, R1 once `Scheduled jobs · profile fixture` shows; `esc`, R2 | standard (normally nothing left) |
| 8 | messaging-browse | none | submit `/messaging`, R1 once `Connectors and messaging · profile fixture` shows; type `/signal`, R2; `enter`, R3 (the `t` of the source is omitted, 9.1) | standard (normally the source's three Esc) |
| 9 | connectors-consent | none | submit `/connectors`, R1 (the consent text) | standard (one `esc`); never `enter` |
| 10 | configure-secret-refused | none | submit `/configure use my openrouter key sk-or-v1-talaria-sentinel-0000000000000000`, R1; `up`, R2 | `ctrl+c`, wait 2.5 s, R3 (clears a recalled draft; on an empty composer it only arms P1-08's 2 s exit window, which then lapses) |

Pass conditions (limits in 9.6; reasons in 9.4). `<n>` is a number of lines; "rows" are the lines the recorded-screen helpers of 9.2 parse.

| # | Step | Reads and pass condition |
|---|---|---|
| 1 | startup | as P1-13 (launch line `'<talaria>' --no-ensure --profile fixture --provider ollama-cloud --model minimax-m3.1-flash; echo SMOKE_EXIT:$?`) |
| 2 | plain-answer | as P1-13; its token is A (the first token drawn in the run) |
| 3 | mcp-browse | P4-04 UAT-1. R1 holds the title and either rows with a state or `No MCP servers in profile fixture yet.`; R2 (the Catalogue tab) holds at least ten rows marked `remote` and no `TransportSpec(`; R3 holds exactly one row naming `deepwiki`; after teardown the screen equals R0. |
| 4 | capabilities-browse | P4-05 UAT-1, screen half. R1 holds the title, category headers and `[x]` or `[ ]` rows; R2 (Toolsets) holds a `<n> tools` count on its rows; R3 (Plugins) holds plugin rows or the empty-state text (constant from P4-05's text table); R4 (after `a`) holds `bundled` rows; R5 shows the idle composer. |
| 5 | memory-open | P4-07 UAT-1. R1 holds the title, `MEMORY PROVIDER`, `BUILT-IN MEMORY`, `SKILL CURATOR`, `built-in only`, both memory file rows (a size or `empty`) and one of `active`, `paused`, `off`; R2 holds no line starting `Memory · profile`. |
| 6 | memory-commands | P4-07 UAT-2. R1 holds `No pending memory writes.` or `Pending memory writes (`; R2 holds `curator: ENABLED`, `curator: PAUSED` or `curator: DISABLED`; neither read holds `Memory · profile` or Talaria's unknown-command text (constant, 10.4 item 5). |
| 7 | cron-browse | P4-08 UAT-1. R1 holds the title and either rows with a schedule and a state (`next in` or `paused`) or `No scheduled jobs in profile fixture. ctrl+n creates one.`; R2 shows the idle composer. |
| 8 | messaging-browse | P4-11 UAT-1 without `t`. R1 holds the title and at least ten platform rows; R2 holds exactly one `Signal` row; R3 holds `SIGNAL_HTTP_URL` as `set` or `not set` and no stored characters (no value text after it). |
| 9 | connectors-consent | P4-11 UAT-2, screen half. R1 holds the consent text naming `fixture` and `enter continue · esc back`; after `esc` the composer returns. (The recording half is XF-5.) |
| 10 | configure-secret-refused | P4-09 UAT-3, first half. R1 holds `That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.` and no overlay title; R2 does not hold the typed line; R3 shows the idle composer. (The recorder halves stay with the source row and XF-3.) |
| 11 | quit | P1-13 REQ-9 with one token: submit `/quit`; within 15 s `SMOKE_EXIT:0` appears and the foreground list is the shell alone; token A appears at least once in the first pane's `recent-unwrapped --lines 3000`; its count is nA. |
| 12 | resume | P1-13 REQ-10 with one earlier token: split the second pane, run `'<talaria>' --no-ensure -c --profile fixture; echo SMOKE_EXIT:$?`; within 30 s the binary is in its foreground list and the screen is unchanged for 2 s; A appears at least once and at most nA times in the second pane's `recent-unwrapped --lines 3000`; submit the prompt of a fresh token C (never equal to A); C appears at least once with the same count on two reads 3 s apart within 60 s; submit `/quit`; `SMOKE_EXIT:0` within 15 s. |
| 13 | capabilities-cli | in the first pane's shell: `'<talaria>' capabilities skills --profile fixture; echo "exit=$?"`, `'<talaria>' capabilities toolsets --json --profile fixture` and `'<talaria>' capabilities plugins --json --profile fixture`. Pass: the first prints a `NAME  STATE  SOURCE  CATEGORY` header and `exit=0`; each JSON parses (`jq .` exits 0) and `jq -e '.profile == "fixture"'` prints `true`; no JSON key is `path`, `install_dir` or `after_install_path`; no JSON string holds the shell's `$HOME` (skipped with a note when the login's home is under four characters). |
| 14 | no-gateway | in the first pane's shell, with `H=<new empty folder>`: `HERMES_HOME=$H '<talaria>' capabilities skills; echo "exit=$?"`, `HERMES_HOME=$H '<talaria>' capabilities nothing; echo "exit=$?"`, `ls -A $H`. Pass: the first says no Hermes gateway is running for this home with a `Next step:` line and `exit=1`; the second prints `talaria capabilities: unknown list nothing; use skills, toolsets or plugins` and `exit=2`; the listing is empty (P4-05 UAT-6). The folder is removed on every path. |

### 9.4 Exact reason texts

`FAIL <step> (<reason>)`; the reasons below are asserted character for character by the tests of 9.9. `<n>` is a number, `<k>` a count, `<name>` a section, row or profile name, `<screen>` one of `MCP servers`, `capabilities`, `memory`, `scheduled jobs`, `messaging`, `connectors`. Every pass condition of 9.3 maps to exactly one reason here, and a step checks its conditions in the order listed, so the first failing condition names the reason.

| Step | Reasons, in check order |
|---|---|
| mcp-browse | `MCP servers did not show "MCP servers · profile fixture" within <n> s` / `the server list shows neither rows nor the empty text` / `the Catalogue tab shows <k> remote rows, not at least 10` / `the Catalogue tab shows "TransportSpec("` / `the filter "deepwiki" shows <k> rows, not 1` / `the screen after closing MCP servers differs from the screen before` |
| capabilities-browse | `capabilities did not show "Capabilities · profile fixture" within <n> s` / `the Skills tab shows no category header or no [x] or [ ] row` / `the Toolsets tab shows no "<n> tools" row` / `the Plugins tab shows neither rows nor the empty text` / `the plugin filter "a" shows no bundled row` / `the composer is not idle after closing capabilities` |
| memory-open | `memory did not show "Memory · profile fixture" within <n> s` / `memory section <name> missing` / `the memory provider row lacks "built-in only"` / `a built-in memory file row shows neither a size nor "empty"` / `the curator row shows none of active, paused, off` / `the memory title is still shown after Esc` |
| memory-commands | `"/memory pending" printed neither "No pending memory writes." nor "Pending memory writes ("` / `"/curator status" printed no curator state` / `a Talaria memory overlay or an unknown-command text answered a Hermes command` |
| cron-browse | `scheduled jobs did not show "Scheduled jobs · profile fixture" within <n> s` / `the job list shows neither rows with a state nor the empty text` / `the composer is not idle after closing scheduled jobs` |
| messaging-browse | `messaging did not show "Connectors and messaging · profile fixture" within <n> s` / `the platform list shows <k> rows, not at least 10` / `the filter "/signal" shows <k> Signal rows, not 1` / `SIGNAL_HTTP_URL is missing from the Signal card` / `SIGNAL_HTTP_URL shows neither "set" nor "not set"` / `SIGNAL_HTTP_URL shows stored characters` |
| connectors-consent | `the connectors consent text is missing or does not name profile fixture` / `the connectors consent text lacks "enter continue · esc back"` / `the composer is not idle after leaving the consent text` |
| configure-secret-refused | `the refusal text of a secret in a /configure request is missing` / `an overlay opened for a secret in a /configure request` / `Up brought the secret line back` / `the composer is not empty after Ctrl+C` |
| capabilities-cli | `capabilities skills printed no header` / `capabilities skills exit was <n>, not 0` / `capabilities --json is not valid JSON` / `capabilities --json .profile is not fixture` / `capabilities --json holds a forbidden key` / `capabilities --json holds the home directory` |
| no-gateway | `capabilities without a gateway did not fail with a next step and exit 1` / `the unknown list text or exit 2 is missing` / `the empty home is not empty (<k> entries)` |
| quit | P1-13's, except that the token reason is `committed output vanished on exit (token A missing)`. Quit has no upper bound: it measures nA itself |
| resume | P1-13's, except `token count after resume: A <a> (first pane <nA>); it must be at least 1 and at most the first pane's count` and `the reply to the token C prompt did not appear and stay stable within 60 s` |
| every screen step (3 to 10) | `the step did not start from an idle composer` (checked first) / `a write banner appeared; the suite must never change anything` (checked on every read) |
| overlay steps only (3, 4, 5, 7, 8, 9) | `the <screen> overlay did not close after 3 Esc` (checked last). Steps 6 and 10 have no such reason: step 6 opens nothing and sends no `esc`, and step 10 fails at once with its own `an overlay opened for a secret in a /configure request` |

The startup and plain-answer reasons are P1-13's. The generic lines (`deadline exceeded in step <name>`, `skipped: <reason>; TALARIA_LIVE_REQUIRE=1`) are P1-13's as well. The suite has no step that may SKIP: a missing precondition is a FAIL of the step that needs it, never a skip. P1-13's only skip is in its approval step, which phase 4b does not run, so a real phase4b run exits 0 (passed), 1 (failed) or 2 (preflight) and never 3; `TALARIA_LIVE_REQUIRE=1` changes nothing for it. The skip and exit-3 behaviour of the shared runner stays covered by the earlier suites' own tests, which this unit does not edit.

### 9.5 Talaria screen texts the steps depend on

One constant per text in `wording4b.rs`; the tests assert the constant, the steps use it. Each source comment describes the behaviour in plain words (for example `// MCP servers: the empty-list sentence`), never a planning code, because the roadmap's rule 7 says not to write planning codes into source comments; this table is the map from constant to source scenario. Each constant is the exact screen text of its source; where a source issue's body abbreviates a text with `...`, the dots are an abbreviation, not screen text, and the full text comes from that issue's specification.

| Constant | Text | Source |
|---|---|---|
| `MCP_TITLE_FMT`, `MCP_EMPTY_FMT` | `MCP servers · profile <name>`, `No MCP servers in profile <name> yet.` | P4-04 UAT-1 |
| `MCP_REMOTE`, `MCP_BAD_TRANSPORT` | `remote`, `TransportSpec(` (the second must be absent) | P4-04 UAT-1 |
| `CAPABILITIES_TITLE_FMT` | `Capabilities · profile <name>` | P4-05 UAT-1 |
| `CAPABILITIES_ON`, `CAPABILITIES_OFF`, `CAPABILITIES_TOOLS_RE`, `CAPABILITIES_BUNDLED` | `[x]`, `[ ]`, `<n> tools`, `bundled` | P4-05 UAT-1 |
| `CAPABILITIES_PLUGINS_EMPTY_FMT` | `You have not installed any plugins in profile <p>. Press a to show the <n> that ship with Hermes, or i to install one.` (T4 in P4-05's specification, section 5.1) | P4-05 UAT-1 |
| `CAPABILITIES_CLI_HEADER`, `CAPABILITIES_UNKNOWN_FMT` | `NAME  STATE  SOURCE  CATEGORY`, `talaria capabilities: unknown list <name>; use skills, toolsets or plugins` | P4-05 UAT-1, UAT-6 |
| `NO_GATEWAY_FMT` | `talaria: no Hermes gateway is running for this home` (and the `Next step:` prefix) | P1-12 UAT-5; P4-05 UAT-6 |
| `MEMORY_TITLE_FMT`, `MEMORY_SECTIONS` | `Memory · profile <name>`; `MEMORY PROVIDER`, `BUILT-IN MEMORY`, `SKILL CURATOR` | P4-07 UAT-1 |
| `MEMORY_BUILT_IN`, `MEMORY_EMPTY`, `MEMORY_CURATOR_STATES` | `built-in only`, `empty`; `active`, `paused`, `off` | P4-07 UAT-1 |
| `MEMORY_PENDING_NONE`, `MEMORY_PENDING_SOME`, `CURATOR_STATUS_RE` | `No pending memory writes.`, `Pending memory writes (`, `curator: (ENABLED\|PAUSED\|DISABLED)` | P4-07 UAT-2 |
| `UNKNOWN_COMMAND` | Talaria's unknown-command text (P2-03's specification; the test author copies it from the built binary) | P4-07 UAT-2 |
| `CRON_TITLE_FMT`, `CRON_EMPTY_FMT`, `CRON_STATES` | `Scheduled jobs · profile <name>`, `No scheduled jobs in profile <name>. ctrl+n creates one.`; `next in`, `paused` | P4-08 UAT-1 |
| `MESSAGING_TITLE_FMT`, `SIGNAL_ROW`, `SIGNAL_URL_FIELD`, `FIELD_SET`, `FIELD_NOT_SET` | `Connectors and messaging · profile <name>`, `Signal`, `SIGNAL_HTTP_URL`, `set`, `not set` | P4-11 UAT-1 |
| `CONNECTORS_CONSENT_FMT`, `CONNECTORS_CONSENT_HINT` | `Connectors run through Nous Research's hosted tool gateway. If profile <p> has no Nous sign-in, Hermes creates a free guest identity for it when you continue.` (P4-11's specification, the consent card); `enter continue · esc back` | P4-11 UAT-2 |
| `MEMORY_FILE_ROWS` | `MEMORY.md`, `USER.md` (the two built-in memory file rows; P4-07 AC-13 greps `MEMORY.md`) | P4-07 UAT-1, UAT-5 |
| `CONFIGURE_SECRET_REFUSAL` | `That request contains something that looks like a key or token. Talaria never sends secrets to a model. Add keys with /providers.` | P4-09 UAT-3 |
| `WRITE_BANNERS` | `Saved.`, `Saved the key`, `MCP servers reloaded.`, `Installed `, `Removed `, `Deleted `, `Cron job '`, `Curator paused`, `Curator resumed`, `Curator running`, `Configured profile`, `Pairing approved`, `Renamed to`, `Signed in to`, `Applying ` | the owners' success texts |

The rule for what becomes a constant: every literal a step matches is a constant in `wording4b.rs`, and this section lists them all. **The source issues and their specifications are the wording authority, never the binary.** A literal not in this section (for example the Skills tab's category header form, or how a cron row shows its schedule) is taken from the owner's specification, checked against the recorded real screen, and added here by a specification correction in the same pull request, never left inline. If the owner's specification and the real screen differ, the constant keeps the specification's text, the step fails on the live run, and the difference is reported to the owner: copying the binary's text alone would certify whatever the binary happens to print. `WRITE_BANNERS` is checked on every read of a screen step: the test author removes from the list any entry that the recorded screens show as ordinary text (for example a column heading) and reports it in the pull request, so the guard fires only on a real banner. A text that differs from its issue is a FAIL of the step (REQ-4); the tester names the constant and the owner issue.

### 9.6 Time budget and defaults

Per-step limits (seconds; `--step-scale` multiplies them, never the deadline):

| Step | Limit |
|---|---|
| startup | 75 |
| plain-answer | 60 |
| mcp-browse | 30 |
| capabilities-browse | 40 |
| memory-open | 30 |
| memory-commands | 40 |
| cron-browse | 30 |
| messaging-browse | 40 |
| connectors-consent | 20 |
| configure-secret-refused | 20 |
| quit | 30 |
| resume | 150 |
| capabilities-cli | 45 |
| no-gateway | 20 |

The limits sum to 630 s (each includes its setup and teardown keys). `startup`, `plain-answer`, `quit` and `resume` are the larger of the value here and P1-13's own, as in phase 4a. The default `--deadline-secs` for `phase4b` is 900 and is fixed: it is not recomputed from the sum. If P1-13's table changes so that the sum passes 750 s, the developer does not raise the deadline: it is a stop condition, reported to the operator on the parent as section 10.3 says. The first live run records the measured times of every step in the journal; a step that needs more than its limit on a healthy machine is a stop condition of the issue. The 630 s sum and the 900 s deadline are this draft's proposal; no run has measured them.

### 9.7 The command steps

Steps 13 and 14 run in the first pane's shell after `resume` (Talaria has quit in both panes), using the same `Terminal` trait as the first steps. They run the binary under test with `HERMES_HOME` unset (step 13) or pointing at the empty folder (step 14). `jq` is used only to parse the JSON in step 13 and is checked by the preflight. The suite never passes `--restart` and never writes into `HERMES_HOME`. The `configure --dry-run` command is not a smoke step: it starts a hidden model session and the free model can misread, which would make the suite flaky; it is P4-09 UAT-5 and XF-4.

### 9.8 Screens and tool output to record

The test author records, from the `main` binary in a throwaway Herdr tab on the live gateway with `--profile fixture`, one screen per read named in 9.3, including each R0 and the screens after each teardown key (about 35 files under `tools/smoke/tests/fixtures/screens/phase4b/`), each scrubbed to `<home>`, `<tmp>` and `<profile>` shapes by the P1-13 scrubber, plus the output of the two command steps. While recording, the author never presses Enter in a card, Ctrl+S, or a confirmation. Recordings are fixtures, never evidence. If a screen cannot be recorded because a child is not yet on `main`, the author hand-builds it from the source scenario's Then line, marks the file `synthetic` in its first line and says so in the pull request; the live run replaces it. The recording also confirms three facts and the pull request reports them: how a platform row and a catalogue row can be told apart from other lines (the helpers of 9.2), that `herdr pane send-text` of `/signal` reaches the messaging filter, and which write banners are ordinary text.

### 9.9 Tests in detail

All under `tools/smoke/tests/`, written in this order (the issue's table).

1. `screen4b.rs`, 14 functions: `overlay_title_found_with_profile`, `mcp_rows_parsed_with_state`, `catalogue_remote_rows_counted`, `catalogue_transport_text_detected`, `capabilities_tabs_rows_parsed`, `capabilities_tools_count_found`, `memory_sections_found_in_order`, `cron_rows_parsed_with_state`, `cron_empty_text_found`, `platform_rows_counted`, `field_state_set_or_not_set`, `consent_found_with_profile`, `write_banner_detected`, `idle_screen_requires_composer_and_no_title`.
2. `steps4b.rs`: for each of the 14 steps two functions, `<step>_passes_on_the_recorded_screen` and `<step>_fails_with_its_exact_reason` (28 in all; the steps in the order of 9.3, step names in snake case), each on `FakeTerminal` with `ManualClock`. The pass test feeds the recorded screens from R0 to the idle screen after teardown and asserts the keys sent (equal to the step's row of 9.3) and the elapsed time. The fail test loops over a table with one row per reason of that step in 9.4 (one changed recording each, including token A missing for `quit`; token A missing and token A printed more than nA times for `resume` (quit has no upper bound); for every screen step (3 to 10) an overlay left open at R0; and, for the overlay steps 3, 4, 5, 7, 8 and 9 only, an overlay that survives 3 Esc), and asserts each reason character for character. `memory_commands` sends no `esc` in any case, and `configure_secret_refused` fails at once with its own overlay reason. Then `a_full_run_returns_to_idle_between_steps` (a full recorded run: at every step boundary the fake screen is the idle screen, and the keys between two steps are exactly the teardown of the first and the setup of the second), `a_failing_step_stops_the_run_and_cleans_up`, `deadline_names_the_step`, `a_missing_precondition_is_a_fail_not_a_skip` (the fake terminal shows no gateway, no `fixture` and no `jq` in turn; each run prints a `FAIL <step>` line and no `SKIP` line and exits 1), `live_require_does_not_change_a_phase4b_result` (the same recorded runs give the same lines and exit codes with and without `TALARIA_LIVE_REQUIRE=1`), `step_scale_leaves_quiet_periods`.
3. `typed_input4b.rs`: `nothing_typed_contains_unattended`, `slash_lines_are_the_allowed_list`, `no_step_passes_restart_or_confirms_anything`, `the_sentinel_is_the_only_secret_typed`, `no_enter_is_sent_while_a_card_or_consent_is_open` (from a card's or the consent text's opening to the next `esc`, the only inputs are the step's own keys; the submit fallback's one `enter` is allowed only after a submission with no card, consent text or title visible), `no_ctrl_s_and_no_ctrl_y_is_sent`, `no_line_runs_hermes_update_or_config_set`. They scan every text and key the fake terminal received in a full run and compare the whole sequence with 9.3.
4. `write_banner4b.rs`: `a_saved_banner_fails_every_step`, `a_reload_or_install_banner_fails_every_step` (inject each banner of `WRITE_BANNERS` into each screen step's read and assert the write-banner reason).
5. `cleanup4b.rs`: `tab_closed_after_pass`, `tab_closed_after_fail`, `tab_closed_on_signal`, `empty_home_removed_after_no_gateway_step`, `scratch_and_empty_home_removed_on_every_path`.
6. `cli4b.rs`, on the binary with `fake-herdr`: `help_lists_phase4b_suite`, `phase4b_refuses_other_profile_model_or_provider`, `missing_jq_is_exit_2_and_creates_nothing`, `deadline_default_is_900_for_phase4b_and_300_for_phase1`, `exit_codes_are_0_1_2_and_never_3` (pass, a failing step and the preflight give 0, 1 and 2; no phase4b input gives 3), `failure_report_names_the_pane_and_is_scrubbed` (the fake pane capture holds a home path, the login name and the sentinel; the printed report and the report file hold `<home>`, `<user>` and `<typed-secret>` in their place and none of the three strings).
7. `fixtures_public_safe4b.rs`: `phase4b_fixtures_hold_no_paths_or_names` (no absolute path, no home directory, no login name, no `--profile` value other than `default`, `fixture` and the deliberately unknown names, and no secret-shaped text other than the sentinel in the recording of step 10).
8. `live_smoke4b.rs`: `live_phase4b_smoke_in_herdr` (`#[ignore]`): runs the suite for real; exit 0 and a line matching `^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$`; fails, never skips, outside Herdr.

**The red run.** The stub commit adds `Suite::Phase4b`, `run_smoke_4b` and the `screen4b.rs` helpers with bodies that panic `not implemented`, and leaves `FromStr`, the help text, the deadline default and the preflight unchanged. So on the red run the `cli4b` tests fail by the old behaviour, every other new test fails by the panic, and a panic of a stub counts as failing for the reason the test names. The one expected pass is `fixtures_public_safe4b::phase4b_fixtures_hold_no_paths_or_names`: it scans already-safe recordings and calls no stub, and making it fail artificially is forbidden. The red-run section of the pull request lists it as the expected pass; any other test that passes on the stubs goes back to the test author.

### 9.10 Cleanup, signals, safety

As P1-13 REQ-12 to REQ-15 (tab closed on every path including SIGINT and SIGTERM, scrubbed reports, no path in any output), plus the empty folder of step 14. The scrubber gains one literal replacement for this suite: the sentinel `sk-or-v1-talaria-sentinel-0000000000000000` is written `<typed-secret>` in every line the suite prints and in every failure report and pane capture it writes. The checks themselves read the unscrubbed pane text, and only output is scrubbed. The suite makes no network call itself and Talaria's only traffic is to the gateway. The gateway calls the model provider for the two ordinary conversation turns (plain-answer, and token C after resume); no step reaches a card or action that calls a provider's key check, a platform or a connector.

### 9.11 README additions

`tools/smoke/README.md` gains a section for the phase 4b suite: what it does, that it never writes, the preconditions (a gateway at the pinned release, `fixture` served with its key, `jq`), the exit codes and how to read a FAIL.

### 9.12 Coverage map: which feature is checked where

| Feature | Smoke step | Scenario rows | Cross-feature |
|---|---|---|---|
| MCP servers (P4-04) | 3 | P4-04 UAT-1 to UAT-6 | XF-1 to XF-3, XF-5 to XF-8, XF-11, OPC-2, OPC-3 |
| Skills, toolsets, plugins (P4-05) | 4, 13, 14 | P4-05 UAT-1 to UAT-6 | XF-1 to XF-2, XF-4 to XF-8, XF-10 |
| Memory (P4-07) | 5, 6 | P4-07 UAT-1 to UAT-6 | XF-1, XF-2, XF-5 to XF-8 |
| Cron (P4-08) | 7 | P4-08 UAT-1 to UAT-6 | XF-1, XF-2, XF-5 to XF-9, OPC-4 |
| Connectors and messaging (P4-11) | 8, 9 | P4-11 UAT-1 to UAT-6 | XF-1 to XF-3, XF-5 to XF-8 |
| Conversational configuration (P4-09) | 10 | P4-09 UAT-1 to UAT-6 | XF-1, XF-3, XF-4, XF-9, XF-10 |
| The phase 4 operator check | none | none | OPC-1 to OPC-5 |

### 9.13 Journal entries

The first live run of the new suite writes a dated entry in `docs/engineering-journal/LEARNINGS.md`: the measured step times, how many teardown Esc presses each overlay needed, which read source each screen step needed (the lagging-read lesson of P1-13), and which write banners were ordinary text. A second entry records what the operator's check taught: which operations needed a tool outside Talaria, if any, and which values did not restore byte for byte.

## 10. Questions the operator owns, and facts not verified

### 10.1 The shape of the record

This unit takes the shape the phase 4a gate was accepted with. Shape A: the four files `phase-4.md`, `phase-4-evidence.md`, `check-phase-4.sh` and `check-phase-4.test.sh` with the checker of section 6. Shape B: the same 67-row table and evidence in the pull request body and its comments, with no checker. A `phase-4a.md` in the merged pull request of PH4A-ACC means A; otherwise, and with no ruling anywhere, B. Draft ADR-0012 ([verification is proportionate](../adr-drafts/0012-verification-is-proportionate.md)) decision 4 bars "per-release receipt schemas, digest-bound evidence trees or release-record tests"; shape B honours it most plainly, shape A is a small script and a table. The operator may rule differently on the parent before the freeze.

Under shape B the lead edits the issue before the freeze, in one commit: it removes the checker and its tests (REQ-9), UAT-1, UAT-2, AC-1's second half, AC-4, AC-9 and AC-12 (AC-14 then counts 4 scenarios), and replaces every criterion that names a phase 4 file or the checker with the commands below. Every command below fails closed: the first line must exit 0, a missing file or a command error is a FAIL, and nothing may name `check-phase-4.sh`.

```bash
gh pr view "$PR" -R "$R" --json body,comments --jq '.body, .comments[].body' > "$X/body.md"   # the saved table and evidence
sed -n '/^| AUTO-1 |/,/^| OP-1 |/p' docs/handoff/2026-10-22-next-steps/units/PH4-ACC.md | cut -d'|' -f2 | sed 's/^ *//;s/ *$//' > "$X/ids.txt"
test -s "$X/body.md" && test "$(wc -l < "$X/ids.txt" | tr -d ' ')" -eq 67; echo "inputs=$?"                    # must print inputs=0
# AC-5 (B): the header lines before the table
sed -n '1,/^| id |/p' "$X/body.md" | grep -cE '^(Candidate: [0-9a-f]{40}|(Hermes|Herdr|OS|Date): [^ <].*)$'   # prints 5
# AC-7 (B): every id exactly once, PASS, on the candidate
awk -F'|' -v c="$CANDIDATE" 'FNR == NR { want[$0] = 1; next } NF == 7 { for (i = 2; i <= 6; i++) gsub(/^[ \t]+|[ \t]+$/, "", $i); if ($2 in want) { seen[$2]++; if ($4 == "PASS" && length($6) >= 7 && index(c, $6) == 1) ok[$2]++ } } END { n = 0; for (k in want) if (seen[k] == 1 && ok[k] == 1) n++; print n }' "$X/ids.txt" "$X/body.md"   # prints 67
```

AC-10 (B) runs its loop unchanged with `"$X/body.md"` in place of `$E/phase-4-evidence.md`. AC-11 (B) runs its four scans with `"$X/body.md"` in place of `$E` and `$E/phase-4-evidence.md`. AC-15 (B) runs its `grep` on `"$X/body.md"`. AC-7 (B) was checked while drafting: 67 on a good table, 66 with one wrong commit or one duplicated row, 0 when every row names another commit. The lead posts the shape on the parent before stage 0; if PH4A-ACC merged with files that match neither shape, stop and ask the operator. Sections 4 to 6 are then reference only.

### 10.2 Decisions, to confirm on the parent issue before the candidate is frozen

1. The shape of the record (10.1).
2. Whether the operator accepts 30 items in the sitting (section 7.5). A standing written opt-in posted on the parent could let an agent tester run the reversible fixture-writing rows (P4-04 UAT-2 and UAT-3, P4-08 UAT-2 to UAT-6, P4-09 UAT-1 and UAT-2, P4-05 UAT-2) and cut the operator's items to about 20. It would change who runs them, not what passes; it is a pre-freeze edit of the issue by 8.1 item 4, and the agents' key rule (8.1 item 6) would be relaxed for those rows only. This draft assumes the operator runs them, as in phase 4a.
3. Which operator items run on the operator's own `fixture` profile: a provider key added and removed (OPC-1), a main model changed and restored, MCP servers and cron jobs created and removed, a skill and a toolset toggled, a plugin installed and removed, `USER.md` emptied (P4-07 UAT-5), the curator run (it may archive unused skills of `fixture`), a messaging credential saved and cleared, a pairing approved and removed. A refusal of any removes its row by 8.1 item 4.
4. Whether the operator wants the sign-in items (P4-04 UAT-4 and UAT-6, P4-11 UAT-4 to UAT-6) in the sitting; they need accounts the operator holds. Dropping them follows 8.1 item 4.
5. The provider for OPC-1 and OPC-3 (8.1 item 7).

### 10.3 The 900 s deadline and the idle limits

The deadline of 900 s (the sum of the step limits is 630 s) and the XF-8 limits (40960 KiB at start, 102400 KiB after, growth 4096 KiB, CPU 1 s per two minutes; the first two are the architecture's budgets, the other two are sanity checks) are part of the parent's required ruling D-6, which the operator posts on the parent as a `Ruling D-6:` comment before the freeze (the parent's AC-12 checks that order). The lead does not freeze without that comment; silence is not a ruling. A rejection is a stop condition of the issue.

### 10.4 Facts not verified, to check at build time and record

1. P4-07 UAT-6 as written starts Talaria without `--no-ensure` against an empty home and expects a "not connected" screen; this unit's safety rule and P1-12 UAT-5 make that impossible (section 7.2 ruling 5). The owner decides; the lead reports it in stage 0.
2. Whether the phase 2a suite's `palette` step still passes with the phase 4a and phase 4b commands in the palette. PH2A-ACC's text says the check is a subset check; AUTO-9 is where that is proven. If it fails, the failure is a defect of the phase 2a suite's wording or of a child and goes to its owner; this unit edits neither.
3. That `snap` moves when an MCP server or a skill changes. The MCP list lives in the profile's configuration; installed skills may live in folders that `snap` does not cover. The cron store is settled, not open: it is `cron/jobs.json` under the profile home, outside `snap` (section 2.2), so rows compare `cronids` instead. If one of them does not move `snap`, say so on the issue: the agent rows' `cmp=0` then proves less for that screen, and the source rows' own read-backs carry the proof. Also check in stage 0 that `snap` is stable between two runs a minute apart with nothing changed.
4. That the `default` profile is served and has a model, so XF-2 can open its screens; and that the screens' titles for `default` follow the pattern of the source scenarios with the name changed (`MCP servers · profile default`). The same for the consent text of `/connectors`.
5. The text of Talaria's unknown-command line (step 6) is not quoted in the source bodies; the test author takes it from P2-03's specification, checks it against the recorded screen and lists it in the pull request (the specification wins, section 9.5). The Plugins empty text T4 and the `/connectors` consent sentence are now quoted in section 9.5 from their owners' specifications.
6. That a `Test:` line of P4-11 UAT-1 sends no request to a platform when the credential is not set; the issue's own text is the authority and the lead asks its owner if the built binary does otherwise.
7. `/configure` mid-turn (XF-6 leaves it out): whether a second model session may start while a turn runs is P4-09's business; if the owner's specification says it is refused, add `/configure` to XF-6 as a `can't run mid-turn` case by a specification correction.
8. Whether `--record` also records the Signal editor's keystrokes: XF-3 checks the recording for the sentinel and the answer is the row's result either way.
9. How a platform row and a catalogue row are told apart from other lines (9.8). If rows cannot be told apart, the test author reports it and the lead asks the owner for a row marker.
10. The restart route and the `hermes config show` facts are verified in [PH4A-ACC](PH4A-ACC.md) section 2.2; recheck them only if `crates/hermes-contract/PINNED` names another release.
11. The real wording of every constant in 9.5 on the built binary.

## Appendix A: the scenarios of the accepted issues, indexed

The six accepted issues' final bodies are the authority; each scenario is rerun as that body writes it (REQ-11). This index is the list the record's rows are built from. Runner: A is an agent tester, O is the operator (section 7.2).

| Row | Title | Runner |
|---|---|---|
| P4-04 UAT-1 | browse the servers and the catalogue | A |
| P4-04 UAT-2 | install deepwiki, apply it, use it, remove it | O |
| P4-04 UAT-3 | an unreachable server and a bad name (negative) | O |
| P4-04 UAT-4 | OAuth sign-in on this machine, cancelled then completed | O |
| P4-04 UAT-5 | keys stay hidden and out of the configuration (negative) | O |
| P4-04 UAT-6 | sign-in over SSH by pasting the address | O |
| P4-05 UAT-1 | browse the three lists | A |
| P4-05 UAT-2 | turn a skill and a toolset off and on again | O |
| P4-05 UAT-3 | search the hub, read the scan, install a skill | O |
| P4-05 UAT-4 | install a catalog plugin and remove it | O |
| P4-05 UAT-5 | refused inputs send nothing (negative) | A |
| P4-05 UAT-6 | no gateway (negative) | A |
| P4-07 UAT-1 | the memory screen opens with three sections and closes cleanly | A |
| P4-07 UAT-2 | Hermes's own /memory and /curator still reach Hermes | A |
| P4-07 UAT-3 | a cancelled delete and a refused switch change nothing | A |
| P4-07 UAT-4 | pause, resume and run the curator on fixture | O |
| P4-07 UAT-5 | deleting USER.md on fixture empties it and says so | O |
| P4-07 UAT-6 | with no gateway the screen says so and sends nothing | A (see 7.2 ruling 5) |
| P4-08 UAT-1 | browse the scheduled jobs | A |
| P4-08 UAT-2 | create, pause, resume and remove a job | O |
| P4-08 UAT-3 | run a job now and read its history | O |
| P4-08 UAT-4 | a bad schedule and an empty prompt are refused (negative) | O |
| P4-08 UAT-5 | running a paused job asks first; declining sends nothing (negative) | O |
| P4-08 UAT-6 | a job's detail card | O |
| P4-11 UAT-1 | browse messaging platforms and test one | A |
| P4-11 UAT-2 | connectors ask before calling Hermes (negative) | A |
| P4-11 UAT-3 | save, review and clear a messaging credential, after a refused value (negative) | O |
| P4-11 UAT-4 | connect, rename and remove a connector account | O |
| P4-11 UAT-5 | a link left open, stopped, then expired (negative) | O |
| P4-11 UAT-6 | a tool off and on; a pairing approved and its approval removed | O |
| P4-09 UAT-1 | turn a setting on and off by asking | O |
| P4-09 UAT-2 | add an MCP server from the catalogue | O |
| P4-09 UAT-3 | a key in the request is refused (negative) | A |
| P4-09 UAT-4 | cancel while thinking; nothing lingers (negative) | A |
| P4-09 UAT-5 | dry run from a script | A |
| P4-09 UAT-6 | refine, inspect and discard | A |
