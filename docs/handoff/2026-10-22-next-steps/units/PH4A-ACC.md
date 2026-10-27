# PH4A-ACC: Phase 4a acceptance: extend the smoke suite to settings, providers, model roles, profiles and the doctor, and run it

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order, and how the result is recorded. This document holds what the people doing the work would otherwise have to guess: the facts about `herdr` the smoke extension relies on, how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (adapted from the phase 2a acceptance unit's and run while drafting, on macOS), the exact steps and pass conditions of the 10 cross-feature scenarios and the operator session, the run order and the rules for failures, the full design of the smoke extension (files, every step with its exact failure text, the screens to record, the tests), and the questions the operator still owns. Appendix A holds the verbatim text of the 36 scenarios of the accepted issues, extracted from their final bodies by script; the source issue's own text wins if it was edited after this document was written.

Sections are numbered so the issue can point at them. Handoff documents are linked relatively: [the roadmap's phase 4](../07-roadmap-and-work-units.md#phase-4--configure-hermes-from-talaria), [the P1-13 roadmap entry](../07-roadmap-and-work-units.md#p1-13--live-smoke-test-in-herdr), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here), [driving Talaria through Herdr by hand](../research/u13-hands-on-talaria-drive.md). The patterns copied are PH2A-ACC and PH3-ACC (record, checker, smoke extension); the smoke suite extended is P1-13.

## 1. What this unit is for, in plain terms

Six issues of the phase 4a capability each passed their own scenarios on their own commit: the exploration that mapped which configuration calls work on the gateway (P4-00), the settings screen (P4-01), providers, keys and sign-in (P4-02), model roles (P4-03), profiles (P4-06) and the doctor (P4-10). A passing child proves its parts. What no child proves is the seams. All five screens read and write the same Hermes profile through one configuration client, so a secret rule or a profile scope that is right in one screen can be wrong in another. All five are commands in one palette, where `/model` and `/models` sit one letter apart. Two of them (the profile list and the doctor) answer the same question, which profile the gateway serves, from the same source, and must agree. And the whole phase is the first that can change the operator's real Hermes profile, so someone has to prove that what looks read-only does not write.

The unit does three things. It extends the smoke suite of P1-13 so that a 17-step run drives the five screens and the two command-line tools in a real pane, writes nothing, and fails with the screen text when something is wrong. The four earlier suites (phase 1, 2a, 2b and 3) each run once more as regressions. It reruns every user acceptance scenario of the six issues (36) and adds 10 scenarios that only exist when features combine, plus one row for the operator's live write suites. And it records the result in two markdown files, checked by a short script.

The roadmap's phase 4 operator check is "add a provider key, add an MCP server, change a profile's model and create a cron job without leaving Talaria or opening a YAML file". The MCP and cron children belong to the phase 4b capability, so the whole check belongs to PH4-ACC. This unit defines the half that exists: the provider key (P4-02 UAT-3), the profile's model (P4-03 UAT-2), the profile checks (P4-06) and a gateway restart (P4-10 UAT-5), as operator items in section 7.4.

The record keeps the ceremony small (the Python client spent 44 percent of its added lines on acceptance records: [02-current-state.md](../02-current-state.md#why-it-ended-up-here), item 4): two markdown files and one checker of about 55 lines, or, under shape B, a table in a pull request (section 10.1). The only other code is the smoke extension, which is a tool, not a record.

## 2. Facts

### 2.1 The `herdr` command line

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) apply unchanged (Herdr 0.9.4, checked by running it on macOS on 2026-10-22; re-check with the Herdr in use and write `herdr --version` in the record header). Three bear on this unit: `pane run` types text and Enter together; `pane read` has sources `visible`, `recent` and `recent-unwrapped`; a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. Whether `visible` or `recent-unwrapped` is stale for a full-screen program is P1-13's H14 **verify** item, settled by its first live run and recorded in its journal entry: read that entry before recording screens, and follow it. The smoke suite reads the source section 9.3 names for each step; the manual rows run `echo READ-CHECK` in the pane first and use the source that shows it. If a source lags or is empty for Talaria, stop and report it; never swap a source silently to make a step pass.

This unit needs `tmux` in two places only: AUTO-9 (the phase 2a suite, which pastes through it) and XF-8 (an 80 by 24 screen). Facts T1 to T5 of the phase 2a unit apply there (`new-session -d -s <name> -x <cols> -y <rows> -c <dir> <command>`, `send-keys -t <name> -l <text>`, `capture-pane -p -t <name>`, `kill-session -t <name>`; checked with `tmux list-commands` on 2026-10-22).

### 2.2 Hermes and Talaria facts used (re-check at the release in `crates/hermes-contract/PINNED` before relying on them)

- `hermes --profile <name> config show` is **not** a proof that nothing changed, and this unit never runs it as one. It prints a summary: its display section prints personality, reasoning, bell and preview settings only, not `display.compact`, `display.skin` or `display.show_cost` (`hermes_cli/config_show.py` lines 64 to 80), its auxiliary summary covers Vision only (lines 136 to 146), and fallback chains and profile descriptions are not printed (`show_config`, lines 167 to 204). Its `load_config` call (line 169) takes a cold path that calls `ensure_hermes_home()` and `backup_config(config_path, "good")` (`hermes_cli/config.py` lines 2347 to 2380), and `backup_config` can create, copy and delete files under `backups/config/` (`hermes_cli/config_backups.py` lines 50 to 66), so it is not even free of filesystem writes. All at Hermes v0.22.4.
- The proof used instead is a private byte snapshot. A named profile's home is `~/.hermes/profiles/<name>` (`hermes_cli/dashboard_procs.py` line 222 at Hermes v0.22.4; `default` is `~/.hermes` itself). Its configuration is `config.yaml` (`get_config_path`, `hermes_cli/config.py` line 431) and its keys are `.env` (`get_env_path`, line 472); the profile description lives in `profile.yaml` (P4-06 specification). The snapshot (`snap`, section 3) records the SHA-256 digest of `config.yaml`, `.env` and every other top-level `*.yaml` and `*.md` file of `fixture`'s home, never their text. It leaves out `auth.json`, because Hermes's credential pool rewrites it in normal use (`agent/credential_pool.py`, `_save_auth_store` calls at lines 1460, 1877 and 1898), and the `backups/`, session and log folders.
- `POST /api/providers/validate` answers `{"ok": true, "reachable": true, "message": ""}` for HTTP 429 as well as for success ("429 = key is valid but rate-limited"), `ok: false` with `That API key was rejected. Double-check it and try again.` for 401 and 403, and `ok: false, reachable: false` when the provider cannot be reached (`hermes_cli/web_routers/config_env.py` lines 1053 to 1064 at Hermes v0.22.4). P4-02 REQ-4 saves the key when `ok` is true. So pressing Enter in a key card with a junk key is a possible write: no agent and no smoke step does it; P4-02 UAT-2 is the operator's (section 7.4).
- `prompt.submit` accepts `unattended` only together with `finite: true`, and then auto-approves the turn (`gateway/session_finite.py` lines 34 to 57 at Hermes v0.22.4). Nothing here sends it.
- A running `talaria` started with `--no-ensure` never runs `hermes gateway ensure`. With an empty Hermes home it prints `talaria: no Hermes gateway is running for this home`, a `Next step: Run` line naming `hermes gateway ensure --json`, and exits 1 (P1-12 UAT-5). For an unknown profile it prints `talaria: the gateway never discovered a profile named <name>`, a `Next step:` line and exits 1.
- The doctor's one action, a gateway restart, is `POST /api/gateway/restart` behind a confirmation. Verified at Hermes v0.22.4 (commit `4d04303818b61bf3aa09278af48356315515965b`): `hermes_cli/web_routers/actions.py` lines 160 to 166 declare the route, call `_spawn_gateway_restart(profile, interrupt=True)` and return `{"ok": true, "pid": <pid>, "name": "gateway-restart"}`. Recheck it only if the candidate pins another release. It interrupts every running turn on every profile the gateway serves. Only the operator runs it, and only the operator reaches its confirmation from the command line (P4-10 UAT-4 and UAT-5).
- The screens' write paths are `PUT /api/config`, the key and endpoint routes, the model-role routes and `profiles.create` (P4-00's finding names the transport per call). They are reached only by confirming a review or pressing Enter in a key card; none of the 17 smoke steps does either.

### 2.3 What the source issues provide

Screen texts are each owned by one source issue and are listed with their sources in section 9.5. The ignored live tests are named in section 7.1 and 7.4. Two behaviours the scenarios rely on that are not Hermes facts: Ctrl+C clears a non-empty draft (P1-08 settled decision 1) and Esc interrupts a running turn (P1-08).

## 3. Freezing and building the candidate

The smoke extension (section 9) is built and merged first, on branch `p4a-acc-smoke`, by a test author and a developer; it is part of the candidate. The lead freezes after the six accepted issues, P1-13, PH2A-ACC, PH2-ACC and PH3-ACC and the extension are merged and closed. Never switch the primary checkout (it may hold untracked material); work in a worktree outside the repository.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p4a-acc                                    # a new folder outside the repository
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p4a-acc-candidate"  # for the bench runs of AUTO-1; deleted at closeout
cargo build --release --workspace --examples             # the one release build: these two commands together,
cargo build --release -p talaria-cli -p talaria-bench       # run once at the freeze and never again on this candidate
shasum -a 256 target/release/talaria                       # the binary every row uses
git switch -c p4a-acc-record                               # the record branch starts at the candidate commit
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
FH=~/.hermes/profiles/fixture                                                     # the fixture profile's home (section 2.2)
plain()    { perl -pe 's/\e\[[0-9;?]*[ -\/]*[@-~]//g; s/\e\][^\a]*\a//g'; }      # strips terminal escape sequences
fleet()    { "$T" fleet list --profile fixture "$@"; }                            # PH3-ACC's helper, for the quiet-fleet check
snap()     { for f in "$FH"/*.yaml "$FH"/*.md "$FH"/.env; do [ -f "$f" ] && printf '%s %s\n' "$(basename "$f")" "$(shasum -a 256 < "$f" | cut -c1-64)"; done; }
```

`snap` prints one line per file: its name and its digest. Its output stays in the row's `$X` folder and is never pasted; only `cmp` results go into evidence.

**Escape sequences.** Every captured text (a `herdr pane read`, a `tmux capture-pane`, a command's output redirected to a file) is passed through `plain` before any `grep`, `^`-anchored match, comparison or width count, for example `vis "$PANE" | plain > "$X/r1.txt"`. `herdr pane read` and `tmux capture-pane -p` without `-e` should already be plain text, and Talaria should not colour output that is not a terminal; `plain` makes the checks independent of both. The smoke suite does the same inside `screen4a.rs` (every helper strips escape sequences first).

**"The composer is visible".** No source issue names one fixed composer text, so the manual rows use a baseline: right after Talaria starts in a row's pane and shows an empty composer, the tester takes one `vis` read and keeps its last three non-blank lines (the composer and the status line) as `$X/idle-tail.txt`. "The composer is visible" means the last three non-blank lines of a later read equal that file (a status-line command, if one were configured, could change them, which is why the operator precondition says none is). The smoke suite's `idle_screen` uses the same rule against the R0 read of step 3, recorded from the real binary.

After opening `/model`, the comparison may use `--allow-readonly-effort-hydration`: the read-only `models.list` call can populate the active model's default reasoning-effort badge (`minimax-m3.1-flash` becomes `minimax-m3.1-flash medium`). The composer rows and all other status fields must remain identical, and the recording must contain zero outbound `session.mutate` and `prompt.submit` frames. This exception applies to P4-02 UAT-1 and XF-3 only; it permits no model change or turn.

**`$BASE`.** The `main` commit the smoke extension started from: just before the `p4a-acc-smoke` pull request merges, the lead runs `git fetch origin; git merge-base origin/main origin/p4a-acc-smoke`, posts the 40-character result on the issue as `BASE=<sha>` and adds the same line to `../acc.env`. The `p4a-acc-smoke` branch is kept until closeout so AC-1 can read it.

`--no-ensure` is on every launch that starts a live session, so Talaria can never start a gateway. Preflight before stage 1 (once, with the Herdr in use): `herdr --version`, `tmux -V`, `jq --version`; run both pane-split forms (the source scenarios' `herdr pane split --current --direction right --cwd . --no-focus` and the helper's) and close the panes they make. A form Herdr rejects is reported on the issue and allowed by REQ-11 change (4). The source scenarios of P4-03 name a 160 by 48 pane: size the pane or the Herdr window to that before running them. If a code path changes after this point the candidate is dead: start again from the top of this section.

## 4. The record

File `docs/acceptance/v0.7.0/phase-4a.md`. The first commit holds this skeleton with every header line filled in with its real value (the checker refuses a header that still holds an angle-bracket placeholder; the values below are examples) and `phase-4a-evidence.md` holding only its title line, so the first run of the real checker is the red run: 60 problems. Commit order: the checker tests with the stub (`test(acceptance): ...`), the empty record (`docs(acceptance): add the empty phase 4a record`), then the real checker (`docs(acceptance): add the phase 4a checker`); at the empty-record commit the stub still prints `FAILED (stub)`, so the 60-problem run is taken at the checker commit, while the record is still all NOT RUN. Row order is the order below; do not add other tables.

```markdown
# Phase 4a acceptance record

Candidate: 0123456789abcdef0123456789abcdef01234567
Hermes: v0.22.4
Herdr: herdr 0.9.4
OS: macos
Date: 2026-11-16

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
| P4-00 UAT-1 | builder | NOT RUN |  |  |
| P4-00 UAT-2 | builder | NOT RUN |  |  |
| P4-00 UAT-3 | builder | NOT RUN |  |  |
| P4-00 UAT-4 | configurer | NOT RUN |  |  |
| P4-00 UAT-5 | builder | NOT RUN |  |  |
| P4-00 UAT-6 | builder | NOT RUN |  |  |
| P4-01 UAT-1 | configurer | NOT RUN |  |  |
| P4-01 UAT-2 | configurer | NOT RUN |  |  |
| P4-01 UAT-3 | developer | NOT RUN |  |  |
| P4-01 UAT-4 | developer | NOT RUN |  |  |
| P4-01 UAT-5 | configurer | NOT RUN |  |  |
| P4-01 UAT-6 | configurer | NOT RUN |  |  |
| P4-02 UAT-1 | configurer | NOT RUN |  |  |
| P4-02 UAT-2 | configurer | NOT RUN |  |  |
| P4-02 UAT-3 | configurer | NOT RUN |  |  |
| P4-02 UAT-4 | configurer | NOT RUN |  |  |
| P4-02 UAT-5 | configurer | NOT RUN |  |  |
| P4-02 UAT-6 | configurer | NOT RUN |  |  |
| P4-03 UAT-1 | configurer | NOT RUN |  |  |
| P4-03 UAT-2 | configurer | NOT RUN |  |  |
| P4-03 UAT-3 | configurer | NOT RUN |  |  |
| P4-03 UAT-4 | configurer | NOT RUN |  |  |
| P4-03 UAT-5 | configurer | NOT RUN |  |  |
| P4-03 UAT-6 | developer | NOT RUN |  |  |
| P4-06 UAT-1 | configurer | NOT RUN |  |  |
| P4-06 UAT-2 | configurer | NOT RUN |  |  |
| P4-06 UAT-3 | configurer | NOT RUN |  |  |
| P4-06 UAT-4 | configurer | NOT RUN |  |  |
| P4-06 UAT-5 | configurer | NOT RUN |  |  |
| P4-06 UAT-6 | scripter | NOT RUN |  |  |
| P4-10 UAT-1 | developer | NOT RUN |  |  |
| P4-10 UAT-2 | developer | NOT RUN |  |  |
| P4-10 UAT-3 | developer | NOT RUN |  |  |
| P4-10 UAT-4 | configurer | NOT RUN |  |  |
| P4-10 UAT-5 | configurer | NOT RUN |  |  |
| P4-10 UAT-6 | developer | NOT RUN |  |  |
| XF-1 | developer | NOT RUN |  |  |
| XF-2 | configurer | NOT RUN |  |  |
| XF-3 | developer | NOT RUN |  |  |
| XF-4 | developer | NOT RUN |  |  |
| XF-5 | scripter | NOT RUN |  |  |
| XF-6 | configurer | NOT RUN |  |  |
| XF-7 | developer | NOT RUN |  |  |
| XF-8 | developer | NOT RUN |  |  |
| XF-9 | developer | NOT RUN |  |  |
| XF-10 | configurer | NOT RUN |  |  |
| OP-1 | configurer | NOT RUN |  |  |
```

Header values: `Candidate:` is `$CANDIDATE`; `Hermes:` is the `tag=` value of `crates/hermes-contract/PINNED` at the candidate (`sed -n 's/^tag=//p' crates/hermes-contract/PINNED` prints `v0.22.4`; the file is exactly two lines, `tag=<tag>` and `commit=<40 hex>`, per P0-03); `Herdr:` is the whole first line `herdr --version` prints, verbatim (the skeleton assumes `herdr 0.9.4`; the checker test's `Herdr: 0.9.4` passes too, because the checker only requires a value); `OS:` is `macos` or `linux`; `Date:` is the day the record is completed.

Rules: `result` is exactly `PASS`, `FAIL`, `BLOCKED (<cause>)` (causes: `environment`, `not built`, `no access`) or `NOT RUN`. A PASS row carries the first 7 or more characters of the candidate in `commit` and the link `phase-4a-evidence.md#<id lower case, spaces to hyphens>` in `evidence`; the row `P4-03 UAT-1` links `phase-4a-evidence.md#p4-03-uat-1`. A row that is not PASS may leave `evidence` and `commit` empty but its evidence section should say why. BLOCKED and NOT RUN never count as a pass; there is no waiver state. Personas and runners are those of the source scenarios (Appendix A) with the rulings of section 7.2; AUTO rows are `builder`.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-4a-evidence.md` starts with `# Phase 4a acceptance evidence` and holds one section per row, in record order:

````markdown
## P4-03 UAT-1

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-11-16.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

A row run more than once keeps one `## <id>` section; each run adds its own line and blocks under it, oldest first, and the last run is the row's result:

````markdown
## XF-7

Candidate: 0123456. Runner: agent tester. Run 1 of 2. Date: 2026-11-16. Not a Talaria defect: the provider answered 429 (rerun approved on the issue).

```text
<run 1 output>
```

Candidate: 0123456. Runner: agent tester. Run 2 of 2. Date: 2026-11-16.

```text
<run 2 output>
```
````

The checker requires, for every PASS row, the `Candidate:` line (7 or more hex characters equal to the start of the candidate, then a full stop or space), exactly one section, and a closed fenced block with at least one non-blank line and at most 60 lines (a fence left open at the end of the file is a problem). The checker reads the last `Candidate:` line of a section and cannot count runs. The reviewer checks what the script cannot: the runner and date line, the run list (`Run <k> of <n>` lines numbered 1 to n), the three-run limit, the operator's comment URL, the row order and persona column, and that every row that is not PASS says why. An operator row holds the operator's words (what they did, the terminal, the result), the URL of the operator's comment on the issue and the read-back of each value they restored.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output (the result lines). Source rows: what the source scenario's Evidence line asks for. XF rows: what section 7.3 names. Smoke rows (AUTO-8 to AUTO-13): the whole output including every step line and the `SMOKE_RESULT:` line, with repeated `NOTE` lines removed (the only lines ever removed), and both tab lists. When the scrubbed output is still over 60 lines, write it as consecutive fenced blocks of at most 60 lines each in the same section, in order, none cut in the middle of a line: the checker measures each block, not the section, and AC-10 reads the `SMOKE_RESULT:` line from whichever block holds it. Screenshots are attached to the operator's comment, not committed. Configuration text (a settings screen with a value) is evidence only as the lines a source scenario names; never paste a whole file, a `snap` digest or `hermes config show` output.

### 5.3 Scrubbing (before anything is pasted)

```bash
scrub() {
  sed -e "s#$HOME#<home>#g" -e "s#$(id -un)#<user>#g" \
      -e "s#${TMPDIR%/}#<tmp>#g" -e "s#$(cd "${TMPDIR:-/tmp}" && pwd -P)#<tmp>#g" \
      -e "s#$PWD#<worktree>#g"
}
herdr pane read "$PANE" --source recent-unwrapped --lines 200 | scrub > "$X/evidence.txt"
```

Then read the result: no home directory, user name (a login name under four characters cannot be scrubbed safely: say so in the section), temporary path, host name, provider key, ticket or key tail beyond the four characters the screen itself shows, and no profile name other than `default` or `fixture`; replace any other with `<profile>`. Three shapes are new in phase 4a: the profile the operator creates in P4-06 UAT-3 (its name appears in the list, the review and the notice), a custom endpoint's URL (P4-02 UAT-6 uses a loopback address and a port: write them as `<loopback>`), and a provider account name in the sign-in item. The checker catches paths only; AC-11 also greps `--profile` values, key shapes and each runner's login name. A clean grep proves only that those shapes are absent: the reading step is the control.

## 6. The checker and its tests

### 6.1 Behaviour

`check-phase-4a.sh [record [evidence]]` (defaults: `phase-4a.md` and `phase-4a-evidence.md` next to the script). It holds the 60 row ids in one `IDS=` line. It prints one line per problem, then `ACCEPTANCE_RESULT: PASSED (<n> of 60 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 60 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. Problems: a row that is not PASS (the line is `<id>: <result>`); a result that is not one of the four forms; a PASS row whose commit is not a prefix of the candidate or is shorter than 7 characters; a PASS row whose evidence link is not the expected one, whose section is missing, empty or longer than 60 lines, or does not name the candidate; a missing, extra or duplicate row; a section that appears twice; a header line that is missing, empty or still a placeholder; an evidence file that ends inside an unclosed fenced block (text in it never counts); a local path anywhere in either file.

The script is a copy of the phase 2a unit's checker (PH2A-ACC; the issue's REQ-9 names the same source) with the file names, the id list and the count changed, plus two checks a review of this unit asked for (an empty header value, an unclosed fence), so the phases stay comparable. Rejected: one parametrised script for all phases, because it would reopen a closed gate's tool. If a command-saving proxy wraps shell commands on the machine, run the checker and its tests through the proxy's pass-through mode or in a plain pane and paste the raw output.

### 6.2 Reference implementation

Written for POSIX `sh` and `awk`; no brace counts in regular expressions; the backtick fence written `\140\140\140`. Exercised while drafting on macOS: the 24 cases below pass, the stub fails every one of them on its stated expectation, and the skeleton record of section 4 ends `FAILED (60 problems, 0 of 60 rows PASS)`. Not run with GNU `awk` or `mawk`: run the cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 4a acceptance record. Usage: check-phase-4a.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-4a.md}
EVI=${2:-$DIR/phase-4a-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-phase-4a: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|AUTO-8|AUTO-9|AUTO-10|AUTO-11|AUTO-12|AUTO-13|P4-00 UAT-1|P4-00 UAT-2|P4-00 UAT-3|P4-00 UAT-4|P4-00 UAT-5|P4-00 UAT-6|P4-01 UAT-1|P4-01 UAT-2|P4-01 UAT-3|P4-01 UAT-4|P4-01 UAT-5|P4-01 UAT-6|P4-02 UAT-1|P4-02 UAT-2|P4-02 UAT-3|P4-02 UAT-4|P4-02 UAT-5|P4-02 UAT-6|P4-03 UAT-1|P4-03 UAT-2|P4-03 UAT-3|P4-03 UAT-4|P4-03 UAT-5|P4-03 UAT-6|P4-06 UAT-1|P4-06 UAT-2|P4-06 UAT-3|P4-06 UAT-4|P4-06 UAT-5|P4-06 UAT-6|P4-10 UAT-1|P4-10 UAT-2|P4-10 UAT-3|P4-10 UAT-4|P4-10 UAT-5|P4-10 UAT-6|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|XF-9|XF-10|OP-1'
awk -v ids="$IDS" -v evi="$EVI" -v pathbad="$PATHS" '
function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
function slug(s) { s = tolower(s); gsub(/ /, "-", s); return s }
function bad(msg) { print msg; problems++; rowbad = 1 }
function iscand(s) { return length(s) == 40 && s ~ /^[0-9a-f]+$/ }
BEGIN {
  n = split(ids, list, "|"); for (i = 1; i <= n; i++) want[list[i]] = 1
  fence = "^\140\140\140"
  while ((getline line < evi) > 0) {            # evidence: "## <id>", a Candidate: line, then fenced blocks with text
    if (line ~ fence) { infence = !infence; if (infence) { blk = 0; btext = 0 } else if (cur != "") { if (blk > longest[cur]) longest[cur] = blk; if (btext > 0) text[cur]++ }; continue }
    if (infence) { if (cur == "") continue; blk++; if (line ~ /[^ \t]/) btext++; continue }
    if (line ~ /^## /) { cur = trim(substr(line, 4)); sect[cur] = 1; nsect[cur]++ }
    else if (cur != "" && line ~ /^Candidate: /) { split(trim(substr(line, 12)), t, /[. ]/); evcand[cur] = t[1] }
  }
  unclosed = infence                              # a block counts only once its closing fence is seen
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
  if (evid != "phase-4a-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-4a-evidence.md#" slug(id))
  if (!(id in sect) || text[id] < 1) bad(id ": the evidence file has no section with a fenced block holding text for it")
  else if (longest[id] > 60) bad(id ": a fenced block of the evidence is longer than 60 lines")
  if (id in sect && iscand(cand) && (length(evcand[id]) < 7 || index(cand, evcand[id]) != 1)) bad(id ": the evidence section must name the candidate (Candidate: <7 hex>.)")
  if (!rowbad) passes++
}
END {
  if (!iscand(cand)) bad("the line Candidate: <40 hex characters> is missing")
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

The test script builds a good record and variations of it in a temporary folder, runs the checker on each and compares the exit code and one expected text. It prints `CASE <name>: ok` or `CASE <name>: FAILED (...)` and a final `CHECK_RECORD_TESTS: PASSED` or `CHECK_RECORD_TESTS: FAILED (<n>)`. It reads the id list out of the checker, so the two cannot drift; `all-pass` expects the text `60 of 60`, so a dropped id fails it.

| # | Case | Builds | Expects (exit; text) |
|---|---|---|---|
| 1 | `all-pass` | 60 PASS rows, a section each | 0; `ACCEPTANCE_RESULT: PASSED (60 of 60 rows PASS, candidate <7 hex>)` |
| 2 | `not-run-fails` | `P4-10 UAT-6` NOT RUN | 1; `P4-10 UAT-6: NOT RUN` |
| 3 | `blocked-fails` | `AUTO-5` `BLOCKED (environment)` | 1; `AUTO-5: BLOCKED (environment)` |
| 4 | `fail-fails` | `XF-2` FAIL | 1; `XF-2: FAIL` |
| 5 | `wrong-commit` | `P4-03 UAT-2` commit `deadbeefdead` | 1; `is not the candidate` |
| 6 | `lower-case-result` | `AUTO-1` result `pass` | 1; `the result must be` |
| 7 | `row-missing` | `OP-1` deleted | 1; `OP-1: row missing` |
| 8 | `unexpected-row` | an added row `ZZ-9` | 1; `unexpected row: ZZ-9` |
| 9 | `duplicate-row` | `AUTO-1` twice | 1; `AUTO-1: appears twice` |
| 10 | `no-candidate` | `Candidate: TBD` | 1; `Candidate: <40 hex` |
| 11 | `no-evidence-section` | no section for `P4-06 UAT-6` | 1; `P4-06 UAT-6: the evidence file has no section` |
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
| 24 | `default-arguments` | the two files next to the script, no arguments | 0; `ACCEPTANCE_RESULT: PASSED` |

The checker ignores heading-like lines inside a fenced block and counts a duplicated section (the phase 1 to 3 scripts do the same). The stub the test author commits first is

```sh
#!/bin/sh
# stub: the real checker replaces this file
IDS='<the same IDS line as in the reference checker>'
echo "ACCEPTANCE_RESULT: FAILED (stub)"; exit 1
```

so every case fails on its expectation (a stub without the `IDS=` line would make every case build an empty record and fail for the wrong reason). Copying the reference test script below is allowed and expected; independence means the author is not the developer who writes the checker.

````sh
#!/bin/sh
# Tests for check-phase-4a.sh. Run: sh docs/acceptance/v0.7.0/check-phase-4a.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-4a.sh"
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
      echo "| $id | developer | $res | phase-4a-evidence.md#$slug | $com |"
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
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (60 of 60 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P4-10 UAT-6" "NOT RUN";               expect not-run-fails 1 "P4-10 UAT-6: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P4-03 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-1' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-1: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P4-06 UAT-6";               expect no-evidence-section 1 "P4-06 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-4a-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
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
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-4a.md" && cp "$T/ev.md" "$T/d/phase-4a-evidence.md"
out=$(sh "$T/d/check-phase-4a.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record, taken at the commit that adds the real checker while the committed record is still the skeleton of section 4 with the real candidate and real header lines: `sh check-phase-4a.sh` must end `ACCEPTANCE_RESULT: FAILED (60 problems, 0 of 60 rows PASS)` and exit 1 (checked while drafting).

## 7. The scenario catalogue

Every row has an id, a persona, a runner, steps, an observable result and the evidence to capture. Runner words: `agent tester` is an agent working in a Herdr pane; `operator` is the human. All commands run at the worktree root unless a row says otherwise. `R=infiquetra/talaria`, `X=$(mktemp -d)` for evidence scratch. No agent row sets `TALARIA_PROBE_WRITES`, passes `--restart`, creates, stops or starts a profile or the gateway, or runs `hermes config set`, `hermes update` or `hermes gateway` anything.

### 7.1 AUTO rows (persona builder; agent tester)

Each row lists its commands, what makes it PASS and what to paste as evidence.

**AUTO-1: project check and the candidate's continuous integration (CI) results.** The `bench` workflow runs on `pull_request` and `workflow_dispatch` (P1-11). GitHub attaches every workflow run to its head commit as check runs, so a dispatch run on the candidate branch should report on the candidate; this is **verified at the first use** (if it does not, the row is BLOCKED (environment) and reported on P1-11). The lead can learn the answer before the freeze: in stage 0 dispatch `bench.yml` on the branch `p4a-acc-smoke` and read `gh api "repos/$R/commits/<its head sha>/check-runs"`; if no `bench` check run is attached, report it on P1-11 then, so the cause is fixed before the candidate exists (a workflow fix after the freeze makes a new candidate).

```bash
gh workflow run bench.yml --ref p4a-acc-candidate -R "$R"
# poll every 10 s, at most 5 minutes, until a run whose headSha is the candidate exists (an older run on the branch does not count)
RUN=$(gh run list -R "$R" --workflow bench.yml --branch p4a-acc-candidate --json databaseId,headSha --jq ".[] | select(.headSha == \"$CANDIDATE\") | .databaseId" | head -1)
gh run watch "$RUN" -R "$R" --exit-status
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
gh api --paginate "repos/$R/commits/$CANDIDATE/check-runs" --jq '.check_runs[] | "\(.name) \(.conclusion)"'
shasum -a 256 target/release/talaria
```

If no run whose `headSha` is the candidate appears within the 5 minutes, the tester polls once more for 5 minutes (a queued run can start late); if there is still none, AUTO-1 is `BLOCKED (environment: no bench run for the candidate)` and is reported on P1-11 (section 8.1 item 3); no candidate is accepted with it BLOCKED.

PASS: `gh run watch` exits 0 on the run for the candidate; the cargo set exits 0; every check-run line ends `success` or `skipped` (a line ending `null` is still running: wait and read again; `failure`, `cancelled`, `timed_out` and `action_required` are a FAIL) and `bench success` is among them. Evidence: run id and `headSha`, exit codes, the check-run lines, the checksum.

**AUTO-2: recorded-frame and recorded-response suites** (tests that play recorded Hermes frames or REST responses back).

```bash
cargo test -p hermes-client --test config_surface_matrix
cargo test -p hermes-client --test rest_fixtures --test mock_rest_merge
cargo test -p hermes-client --test control_verbs --test control_query
cargo test -p talaria-app --test profiles_replay
```

PASS: each exits 0 and prints `^test result: ok\. [1-9][0-9]* passed` (a target that ran no test prints `0 passed` and is a FAIL). A target that does not exist at the candidate is `BLOCKED (not built)`. Evidence: the `test result` lines.

**AUTO-3: the six issues' unit suites** (every test target the issues name, except the replay, recorded-response and live ones).

```bash
cargo test -p hermes-client --test rest_client --test config_api --test providers_api --test model_roles_rest
cargo test -p talaria-core --test settings_schema --test settings_values --test settings_secrets --test settings_filter --test settings_state --test settings_readback --test settings_diff --test settings_command
cargo test -p talaria-core --test providers_keys --test providers_signin --test providers_endpoints --test providers_props --test model_picker
cargo test -p talaria-core --test model_roles_moa --test model_roles_fallbacks --test model_roles_subagents --test model_roles_helpers --test model_roles_state
cargo test -p talaria-core --test profile_names --test profile_roster --test profiles_state
cargo test -p talaria-core --test doctor_diagnose --test doctor_restart
cargo test -p talaria-app --test settings_service --test provider_service --test model_roles_service --test profiles_service --test doctor_service
cargo test -p talaria-tui --test settings_render --test settings_keys --test providers_input --test providers_render --test model_roles_render --test doctor_view
cargo test --release -p talaria-tui --test profiles_render --test profiles_keys
cargo test -p talaria-cli providers_wiring
cargo test -p talaria-cli --test profiles_cmd --test doctor_cli
cargo test -p talaria-cli --test live_model_roles fixture_scrub_is_an_allowlist
```

PASS: every command exits 0, and a command naming k `--test` targets prints at least k lines `test result: ok. N passed` with N at least 1 (a filter-only command prints at least one). A target that no longer exists is `BLOCKED (not built)`. Evidence: the `test result` lines and exit codes. The tests marked `#[ignore]` are run by AUTO-5, by the source rows or by OP-1.

**AUTO-4: performance budgets on the units' own benchmarks and the real release binary.**

```bash
target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"; echo "exit=$?"
grep -cE ' FAIL$' "$X/bench.txt"; grep -cE ' PASS$' "$X/bench.txt"
cargo bench -p talaria-core --bench settings_filter
cargo bench -p talaria-core --bench model_roles_filter
cargo test -p talaria-tui --test settings_render no_frames_when_idle
cargo test --release -p talaria-tui --test profiles_render two_hundred_rows_stay_inside_budget
```

PASS: `exit=0`, the FAIL count 0, the PASS count 1 or more; each bench exits 0 and prints `median_us=<n>` with n at most 2000 (P4-01 REQ-15, P4-03 REQ-22); the two tests pass (P4-01 AC-9, P4-06 AC-7). The flags are P1-11's (`--binary <path>`, `--report <file>`; its specification section 7.3), and the table's first line is `os: <os> arch: <arch> cpus: <n>`. Evidence: the `bench.txt` table from its `os:` line, the two counts, the two `median_us` lines, the exit codes. Budgets: [05-architecture.md](../05-architecture.md#performance-budgets).

**AUTO-5: live read-only suites with no skip.** Operator preconditions hold; each command runs in its own shell and none writes.

```bash
TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_rest_config -- --ignored --nocapture live_reads_on_fixture
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_providers -- --ignored --nocapture live_read_tabs_on_fixture live_rejected_key_is_not_saved 2>&1 | tee "$X/prov.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_model_roles reads_all_sections -- --ignored --nocapture
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_profiles -- --ignored --nocapture live_profiles_list_classifies
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_doctor -- --ignored --nocapture
```

PASS (each test has its own signal): `LIVE_SETTINGS_RESULT: PASSED`, `LIVE_MODEL_ROLES_RESULT: PASSED`, `LIVE_PROFILES_RESULT: PASSED` and `LIVE_DOCTOR_RESULT: PASSED` each appear once, `LIVE_PROVIDERS_RESULT: PASSED` appears twice, `grep -c 'LIVE_PROVIDERS_RESULT: PASSED' "$X/prov.txt"` prints 2, `! grep -q 'sk-or-v1-0000' "$X/prov.txt"`, and no `SKIPPED` anywhere. A skip is `BLOCKED (environment)`, never a pass. The write variants (`TALARIA_PROBE_WRITES=1`) are OP-1, the operator's. Evidence: the result lines.

**AUTO-6: hygiene scans.**

```bash
cargo run -q -p record-fixture -- scan fixtures/frames
! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools
! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input*.rs' ':!tools/smoke/README.md'
! git grep -nE 'end_reason|"unattended"' -- crates/talaria-app/src crates/talaria-cli/src ':!crates/talaria-app/src/bg/*'
! git grep -nE '"(unattended|clone_channels|clone_all|share_auth|no_alias|no_skills)"' -- crates/talaria-app/src/profiles.rs crates/talaria-core/src/profiles
! git grep -nE 'TALARIA_PROBE_WRITES|--restart' -- tools/smoke ':!tools/smoke/tests/typed_input4a.rs' ':!tools/smoke/README.md'
SECRET='(^|[^A-Za-z0-9])sk-[A-Za-z0-9]{8,}|sk-or-v1-[0-9a-f]{8,}|Bearer '
printf '%s\n' 'sk-abcdefgh12345678' | grep -cE "$SECRET"; printf '%s\n' 'sk-or-v1-0000000000' | grep -cE "$SECRET"; printf '%s\n' 'Authorization: Bearer x' | grep -cE "$SECRET"
test -n "$(ls fixtures/rest/model-roles/*.json 2>/dev/null)" && ! grep -rnE "$SECRET|/(Users|home)/" fixtures/rest/model-roles/ fixtures/configuration-surface/
! grep -E '/(Users|home|private|var/folders|tmp)/' fixtures/frames/profiles.jsonl
! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile|nosuch|to|the|fixture-a'
git grep -nE 'config\.set' -- crates/talaria-app/src crates/talaria-cli/src
```

PASS: the `!` lines print nothing; the one `config.set` line prints only hits whose key is not `model` (P4-03 forbids `config.set key=model`; P4-00's finding says what else is allowed), each hit read and listed in the evidence, and any hit that sets `model` is a FAIL; every other line exits 0. The three positive controls (a generic key, an OpenRouter key, a bearer credential) each print 1 before the clean scan counts: the pattern is the same as AC-11's, because `sk-[A-Za-z0-9]{8,}` alone misses `sk-or-v1-...` (the hyphen after `or` ends the match: checked while repairing this unit, the old control printed 0). A control that prints 0 makes the row FAIL whatever the scan shows. `nosuchprofile` and `nosuch` are allowed in the profile scan because the profile and doctor sources type them as deliberately unknown names. A finding is a FAIL reported on this issue and a new candidate follows. Evidence: the command lines that printed something, and `0` counts.

The background detach implementation is excluded from the `end_reason` scan because Phase 3 requires that field there; the profile implementation remains scanned. The source scan also permits prose tokens `to` and `the`, and the synthetic unit-test name `fixture-a`. This does not expand the evidence file's four-name profile allowlist.

**AUTO-7: smoke suite offline tests.**

```bash
cargo test -p smoke
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
cargo run -q -p smoke -- run --help
B=$(mktemp -d)/base; git worktree add "$B" "$BASE"                  # the baseline: the same list at $BASE (section 3)
BT=$(mktemp -d)                                                  # never share the candidate's Cargo fingerprints
(cd "$B" && CARGO_TARGET_DIR="$BT" cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$') > "$X/ignored-base.txt"
git worktree remove --force "$B"; rm -rf "$BT"
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$' > "$X/ignored-now.txt"; diff "$X/ignored-base.txt" "$X/ignored-now.txt"
```

PASS: all tests pass; the ignored list names `live_full_smoke_in_herdr` and `live_phase4a_smoke_in_herdr` among the earlier phases' ignored tests, and has exactly one more line than the list recorded at `$BASE` (the `diff` prints exactly one line, `> live_phase4a_smoke_in_herdr: test` with its module path); the help lists `--talaria`, `--herdr`, `--profile`, `--model`, `--provider`, `--deadline-secs`, `--step-scale`, `--report-dir` and `--suite` with `phase4a` among its values. Evidence: the `test result` line, the ignored lines, the help.

**AUTO-8 to AUTO-13: the smoke suites, back to back at the end of stage 1** with no other live scenario between them (they share one gateway and one free model). The four earlier suites run once each as regression rows, with their owners' preconditions, step counts, deadlines and no-skip rules (P1-13, PH2A-ACC, PH2-ACC, PH3-ACC); the phase 3 suite also needs PH3-ACC's quiet-fleet check before AUTO-11, AUTO-12 and AUTO-13, because the last act of a phase 3 run is a denied approval whose turn may still be working. The check, restated from PH3-ACC section 3: run `fleet | plain` (the helper of section 3) and read it; it must show no `NEEDS YOU` group and no `WORKING` group. If one is there, run it again every 15 s for up to 5 minutes. A session that still needs an answer is answered with deny (resume it with `"$T" --resume <id> --profile fixture --no-ensure` and press Esc on the card) and the row's evidence says so; a session still `WORKING` after 5 minutes is a stop: report it on the issue. The check is a wait, not a scenario.

```bash
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
snap > "$X/before.sum"
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria "$T" > "$X/s1.txt" 2>&1; echo "exit=$?"                        # AUTO-8
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s2.txt" 2>&1; echo "exit=$?"    # AUTO-9
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2b --talaria "$T" > "$X/s3.txt" 2>&1; echo "exit=$?"    # AUTO-10
# quiet-fleet check (PH3-ACC section 3)
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase3 --talaria "$T" > "$X/s4.txt" 2>&1; echo "exit=$?"     # AUTO-11
# quiet-fleet check
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase4a --talaria "$T" > "$X/s5.txt" 2>&1; echo "exit=$?"    # AUTO-12
# quiet-fleet check
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase4a --talaria "$T" > "$X/s6.txt" 2>&1; echo "exit=$?"    # AUTO-13
herdr tab list --workspace "$HERDR_WORKSPACE_ID"; tmux ls 2>&1 | grep -c talaria-smoke
snap > "$X/after.sum"; cmp "$X/before.sum" "$X/after.sum"; echo "cmp=$?"
```

PASS: AUTO-8 prints six `PASS` lines and `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 300; AUTO-9 prints 21 `PASS` lines and the same line with n under 1800; AUTO-10 prints 15 with n under 1500; AUTO-11 prints 12 with n under 1200; AUTO-12 and AUTO-13 each print 17 `PASS` lines and the same line with n under 900; every `exit=0`; no `talaria-smoke-` tab in the second list, the tmux count 0 and `cmp=0` (no configuration or key file of `fixture` changed; the earlier suites' own writes are inside their scratch folders). If an earlier suite's owner changed a count or a limit in its own issue before this candidate, that issue's number wins and the difference is reported here. The defaults are P1-13's (profile `fixture`, model `minimax-m3.1-flash`, provider `ollama-cloud`), so no such flag is passed, and P1-13 makes the `--talaria` path absolute itself. A suite prints `PASSED WITH SKIPS` and exits 3 when a step skips, and with `TALARIA_LIVE_REQUIRE=1` a skip fails the run; neither is a pass: the row is BLOCKED (environment) by section 8.1 item 1. The phase 2a suite's `palette` step checks that the Talaria group lists the local commands as written then; this unit adds five (section 10.4 says what to do if that step fails). A `cmp=1` is a FAIL of AUTO-12 and AUTO-13 (the phase 4a suite must write nothing) unless the tester shows, by rerunning only the four earlier suites with snapshots around each, that an earlier suite caused it; then that suite's row is the FAIL, reported to its owner. Evidence: each whole output (section 5.2), both tab lists and the `cmp=` line (never a digest).

### 7.2 Source rows (36 scenarios)

Each source scenario is rerun exactly as its issue writes it (Appendix A has the text extracted when this unit was written; the issue's own text at the time of the run wins). Who runs it, by ruling of REQ-11:

| Row | Persona | Runner | Source runner line | Covers |
|---|---|---|---|---|
| P4-00 UAT-1 | builder | agent tester | agent tester in Herdr | Success Criteria 1 and 4 |
| P4-00 UAT-2 | builder | agent tester | agent tester in Herdr | Success Criteria 4 |
| P4-00 UAT-3 | builder | agent tester | agent tester in Herdr | Key Question 6 |
| P4-00 UAT-4 | configurer | operator | operator (it writes to a Hermes profile) | Key Questions 2 and 5 |
| P4-00 UAT-5 | builder | agent tester | agent tester in Herdr | Settled decision 2 |
| P4-00 UAT-6 | builder | agent tester | either | Success Criteria 3 |
| P4-01 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-1, AC-4, AC-7 |
| P4-01 UAT-2 | configurer | operator | operator | AC-1, AC-5, AC-8 |
| P4-01 UAT-3 | developer | operator | operator | AC-6, AC-8 |
| P4-01 UAT-4 | developer | agent tester | agent tester in Herdr | AC-6, AC-7 |
| P4-01 UAT-5 | configurer | agent tester | agent tester in Herdr | AC-4, AC-7 |
| P4-01 UAT-6 | configurer | operator | operator | AC-5, AC-8 |
| P4-02 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-1, AC-4, AC-7 |
| P4-02 UAT-2 | configurer | operator | agent tester in Herdr (moved: its Enter can save a junk key, section 2.2) | AC-1, AC-6, AC-7 |
| P4-02 UAT-3 | configurer | operator | operator (it writes the `fixture` profile's key file) | AC-1, AC-7, AC-11 |
| P4-02 UAT-4 | configurer | agent tester | either | AC-2, AC-7 |
| P4-02 UAT-5 | configurer | operator | operator (it uses the operator's provider account) | AC-2, AC-7 |
| P4-02 UAT-6 | configurer | operator | operator, or the agent tester with the operator's written go-ahead (it writes `fixture`) | AC-3, AC-7 |
| P4-03 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-8 |
| P4-03 UAT-2 | configurer | operator | operator, or agent tester with the opt-in | AC-7, AC-8 |
| P4-03 UAT-3 | configurer | operator | operator, or agent tester with the opt-in | AC-7, AC-8 |
| P4-03 UAT-4 | configurer | operator | operator, or agent tester with the opt-in | AC-7, AC-8 |
| P4-03 UAT-5 | configurer | agent tester | agent tester in Herdr (nothing is written) | AC-8 |
| P4-03 UAT-6 | developer | operator | operator (stops and restarts the gateway) | AC-8 |
| P4-06 UAT-1 | configurer | agent tester | agent tester in Herdr | AC-1, AC-7 |
| P4-06 UAT-2 | configurer | operator | operator (it stops and starts a profile's service) | AC-2, AC-8 |
| P4-06 UAT-3 | configurer | operator | operator (it creates a profile) | AC-3, AC-8 |
| P4-06 UAT-4 | configurer | agent tester | agent tester in Herdr | AC-3, AC-7 |
| P4-06 UAT-5 | configurer | operator | operator (it writes to `fixture`) | AC-4, AC-8 |
| P4-06 UAT-6 | scripter | agent tester | agent tester in Herdr | AC-1 |
| P4-10 UAT-1 | developer | agent tester | agent tester in Herdr | AC-8, AC-14 |
| P4-10 UAT-2 | developer | agent tester | agent tester in Herdr | AC-9 |
| P4-10 UAT-3 | developer | agent tester | agent tester in Herdr | AC-10 |
| P4-10 UAT-4 | configurer | operator | agent tester in Herdr (moved: it passes `--restart`, which agents never do) | AC-11 |
| P4-10 UAT-5 | configurer | operator | operator | AC-12 |
| P4-10 UAT-6 | developer | agent tester | agent tester in Herdr | AC-13 |
Order for the 19 agent rows (stage 2): issue order, one live scenario at a time. P4-00 UAT-1, UAT-3 and UAT-5 use `cargo test` in a clean clone of the candidate (the worktree); UAT-2 uses an empty `HERMES_HOME`; UAT-6 is a reading task: the reader (an agent that did not write the finding) answers three questions in a comment on this issue and pastes the answers as evidence. P4-03 UAT-5 writes nothing (its Ctrl+S is refused and `d` discards); P4-06 UAT-1, UAT-4 and UAT-6 write nothing. The 17 operator rows are run in section 7.4. Two rows whose source runner line names an agent tester are the operator's here: P4-02 UAT-2 (pressing Enter on a junk key calls `POST /api/providers/validate`, which answers `ok: true` on HTTP 429, after which P4-02 saves the key: section 2.2; the junk key could land in `fixture`'s key file) and P4-10 UAT-4 (it runs `doctor --restart`, which settled decision 5 of the issue keeps from agents even when the confirmation is refused, because a defect in the refusal would restart the gateway). Agents keep the validate-only live test `live_rejected_key_is_not_saved` (AUTO-5), which never calls the save.

The four allowed changes of the issue's REQ-11, made exact: (1) every `talaria`, `<talaria>` or `"$T"` the scenario runs is written `"$T"`, always (the worktree's `PATH` entry of section 3 is only a backstop against a missed one); (2) `--no-ensure` is added to every launch of the interactive screen that lacks it (`talaria` with or without `-c`, `--resume` or `--profile`), never to a subcommand (`profiles`, `doctor`, `fleet`, `smoke`), which runs as the scenario writes it; (3) and (4) as the issue says.

Rulings that go with the table: (1) the source rows that say "operator, or an agent tester with the opt-in" are the operator's here, so the opt-in `TALARIA_PROBE_WRITES=1` exists only in the operator's shell; (2) P4-02 UAT-4 is "either": an agent tester runs it, with a provider the `fixture` profile is not signed in to, and presses Esc before anything is approved; (3) a source row that needs a state the machine lacks (no provider without a key, no second ollama-cloud model, no `default` profile served) is BLOCKED (environment) with the missing state named; (4) a row's result follows its Then lines, so an output the Then requires is that row's PASS: P4-00 UAT-2 passes when its first run prints `LIVE_CONFIG_SURFACE_RESULT: SKIPPED (` with a reason and exits 0 and its second run, under `TALARIA_LIVE_REQUIRE=1`, fails with that reason; P4-00 UAT-5 passes when it prints `SKIPPED (write opt-in not set)` and `requests sent: 0`. The environment label of section 8.1 item 1 is only for a skip or failure the Then does not ask for.

### 7.3 Cross-feature rows XF-1 to XF-10

Each row: a persona, what seam it covers, steps, a binary pass rule, evidence. Shared set-up: a new `mktemp -d` folder, a Herdr pane `$PANE` made by `pane_new` and a first read check: `submit "$PANE" 'echo READ-CHECK'`, then `vis "$PANE"` or `rec "$PANE"` (whichever shows it) must hold `READ-CHECK` before any Talaria read counts. Talaria is started in a pane with `$T $F` (section 3), then the idle baseline of section 3 is taken. `$X` is the row's scratch folder. Every read and output goes through `plain` before it is checked (section 3).

**XF-1: the doctor and the profile list agree on what the gateway serves.** Persona developer. Covers P4-10 with P4-06 (both read the gateway's served and parked profiles).
- Steps: in the shell pane: `"$T" doctor --profile fixture > "$X/d1.txt"; echo "exit=$?"`; `"$T" profiles fixture > "$X/p1.txt"; echo "exit=$?"`; `"$T" doctor --profile nosuchprofile > "$X/d2.txt"; echo "exit=$?"`; `"$T" profiles nosuchprofile > "$X/p2.txt" 2>&1; echo "exit=$?"`.
- Pass: `grep -cE '^(ok|warn) +Profiles' "$X/d1.txt"` prints 1 and `grep -c 'served' "$X/p1.txt"` prints 1 or more; `grep -cE '^FAIL +Profiles' "$X/d2.txt"` prints 1, `grep -c 'never discovered a profile named nosuchprofile' "$X/d2.txt"` prints 1, and `grep -c 'no profile named nosuchprofile' "$X/p2.txt"` prints 1; the two exit codes of the unknown name are 1 (doctor) and 2 (profiles). The served profile is served in both views, the unknown one is unknown in both.
- Evidence: the four exit lines and the matching lines.

**XF-2: the settings screen, the model roles page and the profile list name the same main model.** Persona configurer. Covers P4-01, P4-03 and P4-06 (one profile, three readers). Needs `fixture`'s main model to be `minimax-m3.1-flash` on `ollama-cloud` (operator precondition).
- Steps: start Talaria in `$PANE`. `submit "$PANE" '/config'`; once `Settings · profile fixture` shows, send text `/` then `model` (no Enter), read V1; `esc`, `esc` (wait 400 ms after each). `submit "$PANE" '/models'`; once the title shows, read V2 (the Main section is the first). `esc`. `submit "$PANE" '/profiles'`; type `/fixture`, read V3; `esc`, `esc`.
- Pass: V1 holds a `model` row whose value is `minimax-m3.1-flash` (`grep -cE '\bmodel\b.*minimax-m3\.1-flash'` prints 1 or more); V2 holds `minimax-m3.1-flash` and `ollama-cloud`; V3 holds `minimax-m3.1-flash · ollama-cloud`. Counted with `grep -c`, each at least 1. No `Failed` text in V2. The pinned config schema names the setting `model`; `profiles.describe` separately nests `model.default`.
- Evidence: the three reads (the lines that match).

**XF-3: the palette lists the five commands, and `/model` stays apart from `/models`.** Persona developer. Covers P4-01, P4-02, P4-03, P4-06, P4-10 with P2-03 (the palette) and P2-06 (the session model picker).
- Steps: start Talaria. For each of `/conf`, `/prov`, `/mode`, `/prof`, `/doct`: send the text without Enter, read, then `ctrl+c` (clears the draft) and read once to confirm an empty composer. Then `submit "$PANE" '/model'`, read V4, `esc`; `submit "$PANE" '/models'`, read V5, `esc`.
- Pass: the five reads hold `/config`, `/providers`, `/models`, `/profiles`, `/doctor` respectively, and the `/mode` read holds both `/model` and `/models`; V4 does not hold `Model roles - profile fixture`; V5 does. After the last `esc` the composer is visible and no turn started. The section 3 read-only effort-badge hydration exception applies, with zero outbound `session.mutate` or `prompt.submit` frames.
- Evidence: the matching lines of the seven reads.

**XF-4: a secret typed anywhere reaches no screen, history file or report (negative).** Persona developer. Covers P4-01 REQ-13, P4-02 (rejected key) and P4-10, P4-06 outputs. The two strings are the only secret-shaped texts allowed (REQ-16): `S1=not-a-real-key`, `S2=sk-or-v1-` followed by 64 zeros.
- Steps: `snap > "$X/before.sum"`. Start Talaria. `submit "$PANE" '/config delegation.api_key=not-a-real-key'` (refused, nothing sent). `submit "$PANE" '/providers'`, type `openrouter`, `enter` (the key card opens), send `S2` by `send-text`, read; `esc` (the card is cancelled: Enter is never pressed in it, because Enter validates and may save, section 2.2), then `esc` until the providers title is gone (at most 3); `submit "$PANE" '/quit'`. In the shell: `grep -c 'not-a-real-key' ~/.talaria/history.jsonl; grep -c 'sk-or-v1-0000' ~/.talaria/history.jsonl` (a missing file counts as 0); `"$T" doctor --json --profile fixture | grep -c 'not-a-real-key\|sk-or-v1-0000'`; `"$T" profiles --json fixture | grep -c 'not-a-real-key\|sk-or-v1-0000'`; `cat "$FH/config.yaml" "$FH/.env" 2>/dev/null | grep -c 'not-a-real-key\|sk-or-v1-0000'` (a count only; the files are never printed); the read of the Talaria pane (`rec`) searched for `sk-or-v1-0000`; `snap > "$X/after.sum"; cmp "$X/before.sum" "$X/after.sum"; echo "cmp=$?"`.
- Pass: every count above prints 0, the key card read (the one taken after `send-text` and before the first `esc`) holds 40 bullets then `…` (`grep -c "$(printf '•%.0s' $(seq 40))…"` prints 1 or more, and `grep -c 'sk-or'` prints 0 on the same read), and `cmp=0`. The history check for `not-a-real-key` is the same as P4-01 UAT-4's.
- Evidence: the counts and the `cmp=` line only.

**XF-5: with no gateway every command fails plainly and starts nothing (negative).** Persona scripter. Covers P4-06 UAT-6, P4-10 UAT-3 and P1-12 UAT-5 together on one empty Hermes home.
- Steps: `H=$(mktemp -d)`. `HERMES_HOME="$H" "$T" doctor > "$X/a.txt" 2>&1; echo "exit=$?"`; `HERMES_HOME="$H" "$T" profiles > "$X/b.txt" 2>&1; echo "exit=$?"`; run `HERMES_HOME="$H" "$T" --no-ensure` inside a pseudo-terminal or throwaway Herdr pane and capture its output to `$X/c.txt` with its exit code (wait at most 30 s); `ls -A "$H" | wc -l`. Both stdin and stdout must be terminals so discovery runs; redirecting this interactive launch directly to a file tests the terminal guard instead.
- Pass: `a.txt` has a `Gateway` row starting `FAIL` followed by the fix line `Fix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.` and exits 1; `b.txt` says no Hermes gateway is running for this home with a `Next step:` line and exits 1; `c.txt` holds `talaria: no Hermes gateway is running for this home` and exits 1; none of the three holds `panic` or `RUST_BACKTRACE`; the folder listing count is 0.
- Evidence: the three outputs (scrubbed) and the count.

**XF-6: each screen names its own profile in two panes.** Persona configurer. Covers P4-01, P4-02, P4-03 with the profile scope shared by all (the screens must never mix profiles). Needs the `default` profile served (operator precondition).
- Steps: two panes `PA` and `PB`. In `PA`: `$T $F`. In `PB`: `$T --no-ensure --profile default`. In each, in turn: `/config` (read, `esc`), `/providers` (read, `esc`), `/models` (read, `esc`), `/doctor` (read, `esc`).
- Pass (the profiles screen is left out on purpose: it lists every profile, so it names both in either pane): `PA` titles hold `profile fixture` for settings, providers and model roles, and `PB` titles hold `profile default`; no `PA` read holds `profile default` and no `PB` read holds `profile fixture` (counted with `grep -c`, 0 each); `/doctor` in both shows its summary line and a `Profiles` row that is `ok`.
- Evidence: the title lines of the eight reads and the four zero counts.

**XF-7: screens opened during a running turn leave the turn running.** Persona developer. Covers the five screens with P1-08 (Esc) and P2-03 UAT-7 (commands while busy).
- Steps: start Talaria; `submit "$PANE" 'Count from 1 to 2000, one number per line, and do not stop early.'`; wait for 15 counting lines (P1-13's rule: a number alone on its line, value 1 to 2000, taken after the last echo of the prompt). Then for each of `/doctor`, `/config`, `/providers`, `/models`, `/profiles`: `submit`, wait 2 s, read; send `esc` only when the first read contains the screen's title (an overlay opened), wait 400 ms, read. An Esc with no overlay would interrupt the foreground turn. Finally read the highest counting value twice, 1 s apart (M0, M1).
- Pass: every attempt's first read shows either the screen's title (opened; the title constants of section 9.5 and `Doctor:`) or a line containing `can't run mid-turn` that was not in the read before the attempt (the busy refusal P2-03 UAT-7 names: a System line containing those words); anything else, including an unchanged screen, is a FAIL. After the last attempt no overlay title remains and the composer input row is visible while counting is still growing (M1 above M0) or a counting line with the value 2000 is visible. The recording's `message.delta` stream must show a single start, with no second restart from 1; the entire sequence need not fit contiguously in the viewport. If the turn ends by itself before the five attempts are done, repeat the row once with `Count from 1 to 5000, one number per line, and do not stop early.` and the value 5000 in place of 2000 (counts toward the three runs).
- Evidence: the ten reads' matching lines and M0, M1.

**XF-8: all five screens fit 80 by 24.** Persona developer. Covers P4-01, P4-02, P4-03, P4-06, P4-10 (P4-03 AC-4 names 80 by 24). Needs `tmux`.
- Steps: `tmux new-session -d -s accxf8 -x 80 -y 24 -c "$X" "unset HERDR_ENV HERDR_PANE_ID HERDR_BIN_PATH; $T $F"`. For each of `/config`, `/providers`, `/models`, `/profiles`, `/doctor` (n = 1 to 5): `tmux send-keys -t accxf8 -l '<command>'`; `tmux send-keys -t accxf8 Enter`; `sleep 3`; `tmux capture-pane -p -J -t accxf8 > "$X/xf8-<n>.txt"` (`-J` joins lines tmux wrapped, so content that overflowed 80 columns shows as one long line); `tmux send-keys -t accxf8 Escape`; `sleep 1`; `tmux capture-pane -p -J -t accxf8 > "$X/xf8-<n>-closed.txt"`; if the screen's title is still in it, send Escape once more, wait 1 s and capture again. `tmux kill-session -t accxf8`.
- Pass: for each open capture: it holds the screen's title text (`Settings · profile fixture`, `Providers · profile fixture`, `Model roles - profile fixture`, `Profiles (`, `Doctor:`), so the top is not clipped; the display-width check `python3 -c 'import sys,unicodedata as u; print(sum(1 for l in sys.stdin.read().split("\n") if sum(2 if u.east_asian_width(c) in "WF" else 0 if u.combining(c) else 1 for c in l.rstrip()) > 80))' < file` prints 0 (it counts terminal columns, two for a wide character, zero for a combining mark, after removing the trailing spaces `-J` keeps). The screen's complete 80-column key hint must be present, with its closing border where applicable; the hint need not be the final non-blank line. Providers places its tab/key hint inside its border, with a closing `╰─…─╯` below; Profiles places hints in its header with details below. Model roles may use `Enter open  Esc close  ? keys`, or wrap its complete footer over `Enter change … Ctrl+R reload` and `Esc close ? keys`; quote the actual complete hint and its recording reference rather than require one contiguous line. For each closed capture: no title text of any of the five screens, and the composer is visible.
- Evidence: the five title lines, the five counts, the five quoted hint lines and the five closed-capture checks.

**XF-9: idle cost after every screen was used.** Persona developer. Covers the five screens, the idle budget of P1-11 and the zero-idle-bytes rule of P4-01 REQ-15.
- Steps: a new folder; start Talaria in `$PANE` with the release binary. After 5 s at an empty composer: `PID=$(herdr pane process-info --pane "$PANE" | jq -r '.result.process_info.foreground_processes[] | select(.cmdline | contains("target/release/talaria")) | .pid')`; sample 0: `R0=$(ps -o rss= -p "$PID")`. Then open and close each screen once (`/config`, `/providers`, `/models`, `/profiles`, `/doctor`: `submit`, wait until its title shows, `esc`). Wait 5 s. Sample 1: `R1`, `C1` (`ps -o time= -p "$PID"`), `V1=$(vis "$PANE")`. `sleep 120`. Sample 2: `R2`, `C2`, `V2`. Convert `time=` with `awk -F: '{ s = 0; for (i = 1; i <= NF; i++) s = s * 60 + $i; print s }'` (macOS prints `M:SS.cc`; the conversion also handles `H:MM:SS`).
- Pass: `R0` at most 40960 (KiB, the 40 mebibyte start budget); `R2` at most 102400 (the 100 mebibyte ceiling); `R2 - R1` at most 4096; `C2 - C1` at most 1 second; `V1` and `V2` identical (`cmp` of the two `plain` reads). Talaria draws nothing at idle except a configured status-line command, which runs on its own interval (P1-10), so the operator precondition is that Talaria's configuration sets no status-line `command` for the run (P1-10's `[status]` table; verify the table name at build time). A line that still changes by itself (a clock, a counter) is quoted, the row is a FAIL and it is reported on P1-10, as PH3-ACC does. The first two limits are the architecture's budgets ("MB" read as mebibytes, as P1-11 decision 7 does); the growth and CPU limits are this issue's own sanity checks, the same as the earlier phases'. A `ps` or `jq` that does not work on the machine makes the row `BLOCKED (environment)`.
- Evidence: the three samples and conversions, the `cmp` result.

**XF-10: a doctor restart with the other screens open (operator).** Persona configurer. Covers P4-10 UAT-5 with P4-01, P4-02, P4-03 and P4-06 (the screens' failure handling when the gateway goes away and returns). Runs last (section 7.4).
- Steps: four panes with Talaria on `fixture`: `/config`, `/providers`, `/models` and `/profiles` open in them. In a fifth pane the operator runs `"$T" doctor --restart --profile fixture; echo "exit=$?"` and types `yes` at the confirmation (the operator accepts that every running turn on every profile is interrupted). Within 60 s of the restart's end, in each of the four panes the operator presses that screen's retry key (`Ctrl+R` on the model roles page; the key the screen's footer names on the others) if the screen shows an error.
- Pass: the doctor prints the line `Restart: Gateway restarted in <n> s (instance <a> → <b>).` with `<a>` and `<b>` different (P4-10 UAT-5) and a second report without a `FAIL` row, and `exit=0`; each of the four screens either kept working or showed a plain error sentence (for the model roles page `Hermes did not answer. Ctrl+R to retry.`) and loaded again after its retry key; no Talaria process exited (`herdr pane process-info` shows each); after the retry each pane shows its screen's title (section 9.5) and its key hint line (the XF-8 rule), which is what "not blank or garbled" means; Esc closes each and the composer is visible.
- Evidence: the operator's comment with the doctor output, one line per pane (the error sentence or `kept working`) and screenshots.

### 7.4 Operator rows (19 items, one sitting)

The operator runs these with the candidate binary `$T`, in this order, and posts one comment per item on this issue, the PH4A-ACC issue (`<id>: PASS`, `FAIL` or `BLOCKED (<cause>)`, with counts and screenshots). Writes come before anything that stops or restarts the gateway, so a half-finished session leaves the least damage. "One sitting" means: the 19 items in this order, on one candidate, with no agent live row run between item 1 and item 19. It may pause between items (even over days); after a pause the operator reads `fleet` and the doctor report before the next item, and a pause never splits an item.

| # | Row | Writes? |
|---|---|---|
| 1 | P4-00 UAT-4 (the probe's write round trip on `fixture`) | yes, restored by the probe |
| 2 | OP-1 (the live write suites, below) | yes, restored by each test |
| 3 | P4-01 UAT-2 (with the digest control below) | yes: `display.compact`, restored in the scenario |
| 4 | P4-01 UAT-3 | yes: `display.compact`, restored in the scenario |
| 5 | P4-02 UAT-2 (a junk key submitted; see below) | only if the provider answers HTTP 429 |
| 6 | P4-02 UAT-3 (replace the `ollama-cloud` key with the same key) | yes: the key file |
| 7 | P4-02 UAT-5 (sign in and out of a provider the operator holds) | yes: the account state |
| 8 | P4-02 UAT-6 (add, test, delete a custom endpoint) | yes, deleted in the scenario |
| 9 | P4-03 UAT-2 (main model) | yes, restored in the scenario |
| 10 | P4-03 UAT-3 (fallback chain) | yes, restored in the scenario |
| 11 | P4-03 UAT-4 (a helper model) | yes, restored in the scenario |
| 12 | P4-06 UAT-5 (description) | yes, restored in the scenario |
| 13 | P4-06 UAT-3 (create a profile; the operator deletes it afterwards) | yes |
| 14 | P4-01 UAT-6 (a change made outside Talaria) | yes: `display.compact` set and reset by hand |
| 15 | P4-10 UAT-4 (a refused restart: `--restart < /dev/null` must exit 3) | no, unless it is defective |
| 16 | P4-06 UAT-2 (stop and start `fixture`'s service) | stops a service |
| 17 | P4-03 UAT-6 (gateway unreachable) | stops the gateway |
| 18 | P4-10 UAT-5 (confirmed restart) | restarts the gateway |
| 19 | XF-10 (a doctor restart with four screens open) | restarts the gateway |

**The digest control.** Before item 1 the operator runs `snap > "$X/sitting-before.sum"` (section 3). Right after the first apply of item 3 (`display.compact` now `true`), `snap > "$X/mid.sum"; cmp "$X/sitting-before.sum" "$X/mid.sum"; echo "cmp=$?"` must print `cmp=1`: this proves the snapshot used by the agents' read-only checks sees a one-setting change. A `cmp=0` here makes item 3 FAIL and every agent `cmp=0` result is then unproven (report it on the issue and stop). After the sitting the operator runs `snap` again and records `cmp` against the first snapshot as information only: Hermes may rewrite a file's layout or comments when it saves (P4-00 UAT-4 asks whether comments survived), so a restored value with different bytes is not a FAIL; the value read-back is the restoration proof.

**P4-02 UAT-2 for the operator.** The operator runs it exactly as written. If `Saved the key for OpenRouter.` appears instead of the rejection (the provider answered HTTP 429, section 2.2), the operator removes the key at once with Ctrl+D and `y` on the OpenRouter row (P4-02 REQ-6), reads the row back as `no key`, and the run is not a Talaria defect (its cause is `provider rate limit`; the lead writes `rerun approved` before a rerun, section 8.1 item 2).

Each row has its own record line, evidence section and comment.

**OP-1: the live write suites.** The operator exports `TALARIA_PROBE_WRITES=1` only in the shell that runs these five commands, one at a time, each with `TALARIA_LIVE_REQUIRE=1`:

```bash
cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_writes_round_trip_on_fixture
cargo test -p hermes-client --test live_rest_config -- --ignored --nocapture live_write_round_trip_on_fixture
cargo test -p talaria-cli --test live_providers -- --ignored --nocapture live_endpoint_round_trip_on_fixture
cargo test -p talaria-cli --test live_model_roles round_trip_writes_on_fixture -- --ignored --nocapture
cargo test -p talaria-cli --test live_profiles -- --ignored --nocapture live_profiles_configure_round_trip
```

PASS: each exits 0 and prints its signal (`LIVE_CONFIG_SURFACE_RESULT: PASSED` with `write-restored` for C10 and C19; `LIVE_SETTINGS_RESULT: PASSED`; `LIVE_PROVIDERS_RESULT: PASSED`; `LIVE_MODEL_ROLES_RESULT: PASSED` with its U1 line; the profiles test's pass line), and none prints `SKIPPED`; each test restores and reads back its own values. The operator unsets `TALARIA_PROBE_WRITES` afterwards. Evidence: the five signal lines and the operator's note.

Operator comments follow the phase 2a format: one comment per item (`<id>: PASS`, `FAIL` or `BLOCKED (<cause>)`, with counts and screenshots); when all 19 pass, one comment on this issue with the line `PHASE 4A OPERATOR ROWS: PASSED (candidate <7 hex>)` (AC-8 reads this issue's comments, never the parent's). An agent copies each comment into the evidence with its URL. The operator restores and reads back every value they changed (`display.compact`, `display.skin`, `display.show_cost`, the main model, the fallback chain, the helper model, the description, the `ollama-cloud` key row, OpenRouter `no key`, any endpoint) and logs the read-back in the comment of the item that changed it.

## 8. Run order, failures and a new candidate

1. Stage 0: build the smoke extension and merge it (section 9). Freeze (section 3).
2. Stage 1 (agent tester, one pane at the worktree root): AUTO-1 to AUTO-7, then AUTO-8 to AUTO-13 back to back.
3. Stage 2 (agent tester): the 19 source rows the operator does not run, in issue order (section 7.2), then XF-1 to XF-9 in order, one live scenario at a time.
4. Stage 3 (operator): section 7.4 (19 items).
The issue's own five scenarios fall in these stages: UAT-3 is AUTO-12 and AUTO-13 themselves (the same two runs, outputs `$X/s5.txt` and `$X/s6.txt`, evidence in those rows' sections; it is not run a third time); UAT-4 runs in stage 2, after XF-9; UAT-5 is stage 3; UAT-1 and UAT-2 run in stage 4, on the record branch, and their results go in the pull request body (REQ-15).

5. Stage 4: write the record and evidence (the first commit is the empty record, section 4), run the checker, open the pull request (documentation only), have it reviewed, merge. The pull request body carries the lines of the issue's REQ-15.

Three identical provider errors in a row (same HTTP status, same message) stop the run: report them on the issue and ask the operator, never retry in a loop. A row may be run up to three times on one candidate; every run goes in its evidence section with why each earlier failure was not a Talaria defect (a provider rate limit, a model that did not follow a plain prompt).

A FAIL stays FAIL until a new candidate passes it. The tester comments the pane text on the issue; the lead files a defect issue through Mission Control for the owner of the source issue (the operator files those found in their rows, or asks the lead to); the fix merges; the lead freezes a new candidate (section 3) and every row is run again; the record is overwritten (git history keeps the old one). A defect is never fixed inside this issue, and no scenario's expected result is edited to make it pass; if a source issue's expected text is wrong, its owner corrects the source issue and this unit's Appendix A is corrected (a correction of the specification is the one change allowed after the freeze).

### 8.1 Rulings that settle questions a builder asked

These settle points where two rules of the issue could be read against each other. They change no pass rule; every one keeps "only PASS is a pass".

1. **A row's label follows its cause, not the exit code.** This item is for a skip the row does not expect; a source row whose Then asks for a skip or a failing exit follows its Then (section 7.2 ruling 4). A suite run that prints `PASSED WITH SKIPS` (exit 3) is BLOCKED (environment). Under `TALARIA_LIVE_REQUIRE=1` a skipped step prints `FAIL <step> (skipped: <reason>; TALARIA_LIVE_REQUIRE=1)` and the run exits 1; the line says `(skipped:`, so the cause is the environment and the row is `BLOCKED (environment: <step> skipped)`, not FAIL. Any other `FAIL <step> (...)` line, including `deadline exceeded in step <name>`, is a FAIL of the row unless item 2 shows it was not a Talaria defect. The checker accepts only the four forms and `BLOCKED (<cause>)` takes any cause text without a closing bracket.
2. **Reruns on the same candidate.** A row may be run up to three times on one candidate. A BLOCKED row is rerun once the cause is fixed (the comment names who fixed what). A FAIL may be rerun only when the tester shows it was not a Talaria defect: the pane text shows a provider error (status and message) or the model not following a plain prompt, while every wording and key of Talaria was as its source issue says; and the lead writes `rerun approved: <row id>: <cause>` on the issue before the rerun. A FAIL that is a Talaria defect (a wording that differs, a key that does nothing, a crash, a wrong state, a write that should not have happened) is final for the candidate: the row is rerun only on a new candidate (the issue's REQ-15). When it is unclear which it is, treat it as a defect, because another candidate costs less than a wrong pass. The record holds the result of the last run; the evidence lists every run (`Run <k> of <n>`). A row never turns PASS on a later run of the same candidate after a Talaria defect.
3. **AUTO-1 when the dispatch run reports nothing on the candidate.** The row is BLOCKED (environment), reported on P1-11. No candidate is accepted with it BLOCKED. Because the answer can be had in stage 0, the operator then has P1-11's workflow fixed before the freeze, or, before the freeze, edits AUTO-1 to drop its bench-run part (settled decision 2).
4. **Dropping a row or a step before the freeze (settled decision 2).** Let N be 60 minus the rows dropped, and S 17 minus the steps dropped. In one commit whose message gives the reason, the lead changes: in the issue, every `60` (Objective, user-visible result, REQ-7, REQ-9, the Tests paragraph, UAT-1, UAT-2, AC-5, AC-9), the row counts `13 + 36 + 10 + 1` (AC-7), `36`, `10`, the 17 operator source rows, the 19 agent source rows and the 19 operator items (REQ-11 to REQ-14, UAT-5, AC-8) as far as they drop, and, for a dropped step, every `17` (REQ-2, REQ-10, UAT-3, the AUTO-12 and AUTO-13 pass rule); here, the section 4 skeleton (rows removed) and the red-run line (`FAILED (N problems, 0 of N rows PASS)`); and in the checker, the `IDS=` line (the only list: the test script builds its rows from it). In the test script: case 1's `58 of 58` becomes `N of N`; a case that names a dropped id (`P4-10 UAT-6`, `AUTO-5`, `XF-2`, `P4-03 UAT-2`, `AUTO-1`, `OP-1`, `P4-06 UAT-6`, `XF-1`, `AUTO-2`, `AUTO-3`, `XF-6`) names another kept id in its place, with the same role; the number of cases stays 24. For a dropped smoke step, the test author deletes that step's functions, screens and wording constants in a commit `test(smoke): drop step <name> (operator decision)`, and the counts of functions (34 step functions, the lists in 9.9) and steps are updated. The lead lists every number changed in a comment on the issue before the freeze. Dropping after the freeze is not possible: it is a new candidate.
5. **A source row whose Hermes behaviour differs on the candidate's Hermes release.** The source issue says which facts to re-verify at the pinned commit. If P4-00's finding says a call moved to another transport, the source rows follow the issue as edited by the planner; a row that only passes with the old transport is a FAIL reported to the owner.
6. **A key card is never submitted by an agent.** An agent row or smoke step that would press Enter in a key card (P4-02's `API key for <label>` card) is out of bounds, whatever the key: Enter validates, and Hermes's validate route answers `ok: true` on HTTP 429, after which P4-02 saves (section 2.2). Such a scenario is the operator's.

### 8.2 Approved corrections on the frozen candidate

The corrections above follow [the first correction and rerun ruling](https://github.com/infiquetra/talaria/issues/255#issuecomment-6443054455): the live profile test selectors, the background detach and source-profile scan exceptions, terminal-backed XF-5 discovery, conditional Esc in XF-7, and actual 80-column hint placement in XF-8. [The second correction and rerun ruling](https://github.com/infiquetra/talaria/issues/255#issuecomment-6444601815) aligns XF-2 with schema key `model`, permits only read-only reasoning-effort badge hydration after `/model`, and records the harness observation repairs for masked captions, number-editor hints, wrapped sign-in instructions, counting-stream observations and wrapped model-role hints. [The Cargo-cache rerun ruling](https://github.com/infiquetra/talaria/issues/255#issuecomment-6444490912) requires a separate temporary target for the baseline worktree in AUTO-7. Every earlier failed run remains in the corresponding evidence section; no product code, expected behavior or frozen binary changed.

[The public-safety correction](https://github.com/infiquetra/talaria/issues/255#issuecomment-6446478367) adds `(^|[^A-Za-z0-9])` before the generic key prefix in the AC-11 regex, preserving all three positive controls while avoiding the `sk-execution` substring in the older Phase 1 record's `terminal-task-execution` prose. The AUTO-6 expression above follows the same corrected pattern; the evidence preserves the actual original run commands. The same comment supplies the section 10.4 at-rest receipt.

## 9. The smoke suite extension

### 9.1 Why a fifth suite, and what it is not

The phase 1 suite proves that a conversation works. The new suite proves that the five configuration screens open, show what Hermes holds, refuse bad input and close, and that the two command-line tools print what their issues say, all on a real terminal against the live gateway. It is black-box like the others: it launches the built binary through `herdr`, sends keys and reads the screen. It is not a replacement for the scenarios (it checks the first screens and the refusals, not the writes), it does not run the earlier suites, and it needs no `tmux`. Above all it writes nothing: no step confirms a review, submits or saves a key, signs in, restarts or creates (REQ-5). The suite is selected with `--suite phase4a` (the earlier extensions made `--suite phase1|phase2a|phase2b|phase3`).

### 9.2 Shape of the code (proposed names; report any change)

In `tools/smoke/src/`: `suite4a.rs` (`run_smoke_4a`, the step table), `steps4a.rs` (one function per step), `screen4a.rs` (pure screen helpers), `wording4a.rs` (the texts, 9.5). Edits to existing files are minimal and listed in the pull request: `suite.rs` (`Phase4a` joins the `Suite` enum PH2A-ACC made and PH2-ACC and PH3-ACC extended, and its `FromStr` accepts `phase4a`; read `unknown_suite_exits_2_with_the_parse_text` in the earlier `cli*.rs` files first: if one pins the parse text exactly, leave that text as it is, because the issue's AC-2 forbids editing that test, and say so in the pull request; `--help` names all five suites), `config.rs` (the `phase4a` deadline default 900; no second enum), `main.rs` (dispatch and the phase 4a preflight), `lib.rs`.

```rust
// suite.rs (existing; one variant and one parser value added)
pub enum Suite { Phase1, Phase2a, Phase2b, Phase3, Phase4a }    // FromStr: "phase1" | "phase2a" | "phase2b" | "phase3" | "phase4a"
// suite4a.rs
pub struct Phase4aConfig { pub empty_home_parent: std::path::PathBuf }   // where the no-gateway step makes its empty folder (default: the system temporary folder)
pub fn run_smoke_4a(term: &dyn Terminal, clock: &dyn Clock, cfg: &SmokeConfig,
                    cfg4a: &Phase4aConfig, stop: &std::sync::atomic::AtomicBool) -> SmokeReport;
```

The phase 4a preflight (in `main`, before anything is created; exit 2, one sentence, no `SMOKE_RESULT` line), in this order: (1) `jq` is not on the `PATH`: `the phase4a suite needs jq on the PATH to parse the doctor and profiles JSON; install jq and run it again`; (2) `--profile`, `--model` or `--provider` was given with a value other than `fixture`, `minimax-m3.1-flash` or `ollama-cloud`: `the phase4a suite checks texts that name profile fixture, model minimax-m3.1-flash and provider ollama-cloud; run it without --profile, --model and --provider`. "Nothing created" means: no Herdr tab (no `tab create` call), no report folder or file, no scratch project and no empty-home folder. `cli4a.rs` observes it through `fake-herdr`'s call log (no `tab create` line) and two empty test folders passed as `--report-dir` and as the temporary root (`TMPDIR`), both still empty after the run. The fixed values are not parameters because the screen titles, the profiles detail and the main-model checks of 9.3 name them; giving the default value explicitly is allowed. The earlier suites keep their own option handling.

`screen4a.rs` (pure functions over screen text, each with unit tests on recorded screens; the 14 names of 9.9): title finders (`settings_title_found(text, profile)`), row parsers (`setting_row(text, key) -> Option<SettingRow { key, value, flags }>`, `providers_row(text, name) -> Option<ProviderRow { name, status, key_tail }>`), `models_sections(text)`, `profiles_rows(text)`, `refusal_found(text, exact)`, `write_banner_found(text)` (true for `Saved `, `Not restarted`, `Gateway restarted`), `doctor_rows(text)`, `doctor_json(text) -> (format, checks)`, `doctor_summary(text) -> (ok, warn, fail)`, `idle_screen(text) -> bool` (no title of 9.5 visible and the composer shown).

### 9.3 Flow, step by step

Common to all steps: a step begins when the previous one passed; a step reads with `visible` unless the line says otherwise; a failing step ends the run with its FAIL line of 9.4 and the pane capture; every read of a screen step also runs `write_banner_found` and fails with the write-banner reason if it is true. The write-banner guard reports a write after it happened; what prevents a write is the key list below.

**The idle state and the key rule.** Steps 3 to 12 (the screen steps) each start and end in the idle state: Talaria running in the first pane, no overlay title of 9.5 visible, an empty composer, no turn running. A screen step first reads the pane (R0) and fails with `the step did not start from an idle composer` unless `idle_screen(R0)` is true. It then sends, in order: its setup keys (the slash line that opens the screen and the source scenario's Given state), the source scenario's When keys up to its last read, and the standard teardown. The standard teardown is: while the screen's title is visible, send `esc`, wait 400 ms and read (`visible`, like every screen-step read), at most 3 times; if the title is still visible after the third, fail with `the <screen> overlay did not close after 3 Esc`. When the source keys already closed the overlay, the teardown sends nothing. Step 5 is the one exception: it opens no overlay, so its teardown is `ctrl+c`, a 2.5 s wait and a read (its row below); the issue's REQ-2 says the same. A source scenario's `/quit`, and any key after its last read, is omitted, because the run continues. Nothing else is sent. "Type `x`" is `herdr pane send-text` of `x` with no Enter; "submit `x`" is `send-text` of `x`, then `send-keys enter`; other key names are `herdr pane send-keys` names; a 400 ms wait follows every `esc`. Every key and text in this table, in this order, is the allowed list of `typed_input4a.rs`.

| # | Step | Setup keys | Source keys and reads | Teardown |
|---|---|---|---|---|
| 3 | config-browse | submit `/config`; R1 once `Settings · profile fixture` shows | `right`, `right`; type `/`, type `compact`, R2; `esc`; type `/`, type `api_key`, R3; `esc`, `esc`, R4 | standard (normally nothing left) |
| 4 | config-invalid-input | submit `/config`, wait for the title; type `/`, type `resume_exchanges`, `enter` (the Given: filtered, cursor on `display.resume_exchanges`) | `enter`, type `abc`, `enter`, R1; `esc`, R2; type `a`, R3 | standard |
| 5 | config-secret-refused | none | submit `/config delegation.api_key=not-a-real-key`, R1; `up`, R2 | `ctrl+c`, wait 2.5 s, R3 (Ctrl+C clears a recalled draft; on an empty idle composer it only arms P1-08's 2 s exit window, which then lapses) |
| 6 | providers-tabs | none | submit `/providers`, R1 once `Providers · profile fixture` shows; `tab`, R2; `tab`, R3 | standard |
| 7 | providers-key-masked | submit `/providers` (it opens on Keys, the Given of P4-02 UAT-2), wait for the title | type `openrouter`, R1; `enter`, R2; type the junk key (no Enter), R3; `esc`, R4 | standard |
| 8 | models-page | none | submit `/models`, R1 once the title shows; type `2`, R2; type `3`, R3; type `4`, R4; type `5`, R5 | standard |
| 9 | models-bad-preset | submit `/models`, wait for the title; type `5`, `left` (the Given: Mixture of Agents with focus on the preset list; P4-03's key table moves focus between the list and the detail pane with Left and Right, and its view starts on the list, `focus_detail` false; `left` makes sure) | type `a`, type `uat-test`, `enter`, `right`, type `a`, `esc`, `ctrl+s`, R1; `esc`, R2; type `d`, R3; submit `/models`, wait for the title, type `5`, R4 | standard |
| 10 | profiles-list | none (R0 is the screen before) | submit `/profiles`, type `/fixture`, R1; `enter`, R2; `esc`, `esc`, R3 | standard (normally nothing left) |
| 11 | profiles-bad-names | submit `/profiles`, wait for the header, RL (the list) | type `n`, type `Bad Name!`, `enter`, R1; `ctrl+u`, type `default`, `enter`, R2; `ctrl+u`, type ` Fixture `, `enter`, R3; `esc`, R4 | standard |
| 12 | doctor-overlay | none | submit `/doctor`, wait for a line starting `Doctor:`; `down`, `down`, `enter`, R1; `esc`, R2 | standard (normally nothing left) |

Pass conditions (limits in 9.6; reasons in 9.4):

| # | Step | Reads and pass condition |
|---|---|---|
| 1 | startup | as P1-13 (launch line `'<talaria>' --no-ensure --profile fixture --provider ollama-cloud --model minimax-m3.1-flash; echo SMOKE_EXIT:$?`) |
| 2 | plain-answer | as P1-13; its token is A (the first token drawn in the run) |
| 3 | config-browse | P4-01 UAT-1. R1 holds the title and the categories `general`, `agent` and `terminal` in that order; R2 holds a `display.compact` row with a boolean, `set` or `default`, and `may need a restart`; R3 holds `delegation.api_key` whose value is exactly `(not set)` or only bullets (`•`, optionally then `…`), as the source Then allows both; any other value is plain text and fails; R4 equals R0; the title appears within 3 s of Enter (the budget of the source; the limit of the step is looser). |
| 4 | config-invalid-input | P4-01 UAT-5. R1 holds `Enter a whole number.` with the editor still open; R2 has no `*` mark; R3 holds `No changes to review.`. |
| 5 | config-secret-refused | P4-01 UAT-4, first half. R1 holds the exact refusal text and no overlay title; R2 and R3 hold `not-a-real-key` nowhere; R3 shows the idle composer. (The quit, restart, search and history-file halves stay with the source row and XF-4.) |
| 6 | providers-tabs | P4-02 UAT-1, reads 1 to 3. R1: `Providers · profile fixture`; `Ollama Cloud` with a status starting `ready ·` and `key …` plus four characters; `OpenRouter` with `no key`; R2: `not signed in` or `signed in` on each row; R3: `+ Add an endpoint`. SKIP, the only skip of the suite, when every other condition holds and OpenRouter shows a key status instead of `no key` (the operator precondition is missing); the run goes on, so the suite ends `PASSED WITH SKIPS` (exit 3, or a FAIL under `TALARIA_LIVE_REQUIRE=1`) and the row is BLOCKED (environment) by 8.1 item 1. |
| 7 | providers-key-masked | P4-02 UAT-2 up to the paste, with the Enter after the paste left out (8.1 item 6). R1 shows the `OpenRouter` row (its status text is S); R2 shows `API key for OpenRouter`; R3 shows exactly 40 `•` then `…`, and neither `sk-or` nor `0000` anywhere; R4 has no `API key for OpenRouter` and the OpenRouter row's status equals S. Nothing is sent to Hermes: P4-02 sends `POST /api/providers/validate` only on Enter. Runs the same whether or not OpenRouter has a key. |
| 8 | models-page | P4-03 UAT-1. R1: title `Model roles - profile fixture` and Main shows a model; R3 (Helpers) lists `Vision` first and `Curator` twelfth; R4 (Subagents) shows a route and a chain mode; R5 (Mixture of Agents) shows a preset or the notice; no read says `Failed`. |
| 9 | models-bad-preset | P4-03 UAT-5. R1: `preset 'uat-test' reference 1: provider and model are required` and no review screen (`Review changes to `, the start of `MODELS_REVIEW_TITLE_FMT`, absent); R2: the apply, discard or stay question (`MODELS_UNSAVED_PROMPT_FMT`); R3: no `Model roles - profile fixture` title; R4: no `uat-test`. Never press `a` on the question. |
| 10 | profiles-list | P4-06 UAT-1, screen half. R1: header starts `Profiles (`; `fixture` shows `served` and `minimax-m3.1-flash · ollama-cloud`; R2: the detail shows `talaria --profile fixture` and a `Skills:` line; R3 equals R0. |
| 11 | profiles-bad-names | P4-06 UAT-4, screen half. R1, R2 and R3 hold, in that order, the three full texts of `PROFILES_NAME_REFUSALS` (9.5), each under the Name field with focus still there; R4 equals RL. Never press Enter on a review. |
| 12 | doctor-overlay | P4-10 UAT-2. R1: `Doctor: <a> ok, <w> warnings, <f> failures`, seven rows, the key hint line as the last overlay line, `running: ` and `built for: v0.22.4 (4d043038)` (the version text comes from the pin at the candidate: the constant reads `crates/hermes-contract/PINNED`'s tag and commit prefix at run time, never a literal); R2 shows the composer and no `Doctor:`. Never press `R`. |
| 13 | quit | P1-13 REQ-9 with one token: submit `/quit`; within 15 s `SMOKE_EXIT:0` appears and the foreground list is the shell alone; token A appears at least once in the first pane's `recent-unwrapped --lines 3000`; its count is nA. Phase 4a has no interrupt step, so there is no token B; P1-13's own two-token check is unchanged. |
| 14 | resume | P1-13 REQ-10 with one earlier token: split the second pane, run `'<talaria>' --no-ensure -c --profile fixture; echo SMOKE_EXIT:$?`; within 30 s the binary is in its foreground list and the screen is unchanged for 2 s; A appears at least once and at most nA times in the second pane's `recent-unwrapped --lines 3000` (zero: history lost; more: printed twice); submit the prompt of a fresh token C (never equal to A); C appears at least once with the same count on two reads 3 s apart within 60 s; submit `/quit`; `SMOKE_EXIT:0` within 15 s. |
| 15 | profiles-cli | in the first pane's shell: `'<talaria>' profiles fixture default`, `'<talaria>' profiles --json fixture` and `'<talaria>' profiles nosuchprofile; echo "exit=$?"`. Reads: `recent-unwrapped`. Pass: a header, a `fixture` row and a `default` row; the JSON parses (`jq .` exits 0); the unknown name prints `talaria profiles: no profile named nosuchprofile` and `exit=2` (P4-06 UAT-1 and UAT-4, command half). |
| 16 | doctor-cli | in the first pane's shell: `'<talaria>' doctor --profile fixture; echo "exit=$?"`, `'<talaria>' doctor --json --profile fixture`, `'<talaria>' doctor --profile nosuchprofile; echo "exit=$?"`, `'<talaria>' doctor --profile ""; echo "exit=$?"`. Pass: seven rows beginning `ok` or `warn`, a `Result:` line, `exit=0`, no home directory in the output; the JSON `.format` is `talaria-doctor-v1` with 7 checks; the unknown name gives `Profiles` as `FAIL` with `the gateway never discovered a profile named nosuchprofile`, `Attach` as `skip` and `exit=1`; the blank profile prints a usage error and `exit=2` (P4-10 UAT-1 and UAT-6). The home-directory check reads `$HOME` from the shell with `echo $HOME` and searches for it; if the login's home is shorter than four characters the check is skipped and the step notes it. |
| 17 | no-gateway | in the first pane's shell, with `H=<new empty folder>`: `HERMES_HOME=$H '<talaria>' doctor; echo "exit=$?"`, `HERMES_HOME=$H '<talaria>' profiles; echo "exit=$?"`, `ls -A $H`. Pass: `Gateway` starts `FAIL` and the next line is exactly ``Fix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.``, `exit=1`; the profiles command says no Hermes gateway is running for this home with a `Next step:` line and `exit=1`; the listing is empty (P4-10 UAT-3 and P4-06 UAT-6). The folder is removed on every path. |

### 9.4 Exact reason texts

`FAIL <step> (<reason>)`; the reasons below are asserted character for character by the tests of 9.9. `<n>` is a number, `<k>` a count, `<name>` a category, section, row or profile name, `<screen>` one of `settings`, `providers`, `model roles`, `profiles`, `doctor`. Every pass condition of 9.3 maps to exactly one reason here, and a step checks its conditions in the order listed, so the first failing condition names the reason.

| Step | Reasons, in check order |
|---|---|
| config-browse | `settings did not show "Settings · profile fixture" within <n> s` / `settings category <name> missing` / `display.compact row missing after the filter "compact"` / `display.compact row lacks a value, set or default, or "may need a restart"` / `delegation.api_key missing after the filter "api_key"` / `delegation.api_key shows plain text, not (not set) or bullets` / `the screen after closing settings differs from the screen before` |
| config-invalid-input | `no whole-number refusal after "abc"` / `the number editor closed on invalid input` / `a "*" mark remained after Esc` / `"No changes to review." missing after the refused edit` |
| config-secret-refused | `the refusal text of a secret on the command line is missing` / `a settings overlay opened for a secret` / `Up brought the secret line back` / `the composer is not empty after Ctrl+C` |
| providers-tabs | `providers did not show "Providers · profile fixture" within <n> s` / `Ollama Cloud is not ready` / `Ollama Cloud shows no key tail "key …" plus four characters` / `the Accounts tab shows no sign-in state on row <name>` / `the Endpoints tab has no "+ Add an endpoint" row`; skip reason: `OpenRouter already has a key on profile fixture (operator precondition: no OpenRouter key)` |
| providers-key-masked | `the OpenRouter row is missing after the filter "openrouter"` / `the key card for OpenRouter did not open` / `the key card showed the typed key` / `the key card did not show 40 bullets and "…"` / `the key card did not close on Esc` / `the OpenRouter row changed after a cancelled key card` |
| models-page | `model roles did not show its title within <n> s` / `section <name> says Failed` / `Main shows no model` / `Helpers does not list Vision first and Curator twelfth` / `Subagents shows no route or no chain mode` / `Mixture of Agents shows neither a preset nor the notice` |
| models-bad-preset | `the incomplete-preset refusal is missing` / `a diff was offered for an incomplete preset` / `Esc did not ask to apply, discard or stay` / `d did not close the model roles page` / `a preset "uat-test" exists after discard` |
| profiles-list | `profiles header missing` / `profile fixture is not served` / `the fixture row lacks "minimax-m3.1-flash · ollama-cloud"` / `the detail of fixture lacks "talaria --profile fixture"` / `the detail of fixture lacks a "Skills:" line` / `the list changed after Esc` |
| profiles-bad-names | `name refusal <k> of 3 missing or out of order` / `focus left the Name field` / `the profile list changed after the refused names` |
| doctor-overlay | `doctor summary line missing within <n> s` / `doctor shows <k> rows, not 7` / `the key hint line is not the last overlay line` / `"running:" detail missing` / `"built for:" detail missing` / `doctor did not close on Esc` |
| quit | P1-13's, except that the token reason is `committed output vanished on exit (token A missing)` |
| resume | P1-13's, except `token count after resume: A <a> (first pane <nA>); it must be at least 1 and at most the first pane's count` and `the reply to the token C prompt did not appear and stay stable within 60 s` |
| profiles-cli | `profiles command printed no header` / `profiles command printed no row for <name>` / `profiles --json is not valid JSON` / `the unknown-profile text is missing` / `unknown profile did not exit 2` |
| doctor-cli | `doctor printed <k> rows, not 7` / `doctor row <name> is not ok or warn` / `doctor printed no Result: line` / `doctor exit was <n>, not 0` / `doctor output contains the home directory` / `doctor --json format is not talaria-doctor-v1 with 7 checks` / `unknown profile did not give FAIL Profiles and exit 1` / `blank profile did not exit 2` |
| no-gateway | `doctor without a gateway did not fail with the documented fix` / `profiles without a gateway did not exit 1 with a next step` / `the empty home is not empty (<k> entries)` |
| every screen step | `the step did not start from an idle composer` (checked first) / `the <screen> overlay did not close after 3 Esc` (checked last) / `a write banner appeared; the suite must never change anything` (checked on every read) |

The startup and plain-answer reasons are P1-13's. The generic lines (`deadline exceeded in step <name>`, `skipped: <reason>; TALARIA_LIVE_REQUIRE=1`) are P1-13's as well.

### 9.5 Talaria screen texts the steps depend on

One constant per text in `wording4a.rs`; the tests assert the constant, the steps use it. Each source comment describes the behaviour in plain words (for example `// Settings: refusal of a non-number typed into a number field`), never a planning code, because the roadmap's rule 7 for every unit says not to write planning codes into source comments; this table is the map from constant to source scenario. Each constant is the exact screen text of its source; where a source issue's body abbreviates a text with `...`, the dots are an abbreviation, not screen text, and the full text comes from that issue's specification.

| Constant | Text | Source |
|---|---|---|
| `SETTINGS_TITLE_FMT` | `Settings · profile <name>` | P4-01 UAT-1 |
| `SETTINGS_NOT_SET` | `(not set)` (the other allowed form is bullets) | P4-01 UAT-1 |
| `SETTINGS_NUMBER_REFUSAL`, `NO_CHANGES_TO_REVIEW` | `Enter a whole number.`, `No changes to review.` | P4-01 UAT-5 |
| `SETTINGS_SECRET_REFUSAL` | `delegation.api_key is a secret; set it in /config, not on the command line. Nothing was sent.` | P4-01 UAT-4 |
| `PROVIDERS_TITLE_FMT`, `PROVIDERS_NO_KEY`, `PROVIDERS_ADD_ENDPOINT` | `Providers · profile <name>`, `no key`, `+ Add an endpoint` | P4-02 UAT-1 |
| `PROVIDERS_KEY_CARD_TITLE_FMT` | `API key for <label>` | P4-02 UAT-1 read 5, REQ-4 |
| `PROVIDERS_KEY_MASK` | 40 `•` then `…` | P4-02 UAT-2, REQ-4 |
| `MODELS_TITLE_FMT` | `Model roles - profile <name>` | P4-03 UAT-1 |
| `MODELS_PRESET_INCOMPLETE_FMT` | `preset '<name>' reference 1: provider and model are required` | P4-03 UAT-5 |
| `MODELS_REVIEW_TITLE_FMT`, `MODELS_UNSAVED_PROMPT_FMT` | `Review changes to <Section> - profile <name>`; `You have unapplied changes in <Section>. a apply · d discard · Esc stay` (P4-03 specification sections 4.4d and 4.4c; step 9 checks `Review changes to ` absent and the question with the Mixture of Agents section's name as P4-03 writes it) | P4-03 UAT-5 |
| `PROFILES_HEADER`, `PROFILES_SERVED` | `Profiles (`, `served` | P4-06 UAT-1 |
| `PROFILES_NAME_REFUSALS` | `Use lower-case letters, digits, - or _, starting with a letter or digit, up to 64 characters.`; `default is reserved by Hermes. Choose another name.`; `A profile named fixture already exists.` | P4-06 UAT-4; full texts from the P4-06 specification's name errors (its line 288) |
| `PROFILES_UNKNOWN_FMT` | `talaria profiles: no profile named <name>` | P4-06 UAT-4 |
| `DOCTOR_SUMMARY_RE`, `DOCTOR_FORMAT` | `Doctor: <a> ok, <w> warnings, <f> failures`, `talaria-doctor-v1` | P4-10 UAT-1, UAT-2 |
| `DOCTOR_FIX_NO_GATEWAY` | ``Fix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.`` | P4-10 UAT-3 |
| `DOCTOR_UNKNOWN_PROFILE_FMT` | `the gateway never discovered a profile named <name>` | P4-10 UAT-6 |

The rule for what becomes a constant: every literal a step matches is a constant in `wording4a.rs`, and this section lists them all. The table above holds the sentences; the short labels the steps also match are constants too, named here with their source: `SETTINGS_CATEGORIES` (`general`, `agent`, `terminal`), `SETTINGS_SET`, `SETTINGS_DEFAULT`, `SETTINGS_RESTART_FLAG` (`set`, `default`, `may need a restart`) from P4-01 UAT-1; `PROVIDERS_OLLAMA`, `PROVIDERS_OPENROUTER` (`Ollama Cloud`, `OpenRouter`), `PROVIDERS_READY` (`ready ·`), `PROVIDERS_KEY_TAIL` (`key …`), `PROVIDERS_SIGNED_IN`, `PROVIDERS_NOT_SIGNED_IN` (`signed in`, `not signed in`) from P4-02 UAT-1; `MODELS_FAILED` (`Failed`), `MODELS_HELPERS_FIRST`, `MODELS_HELPERS_TWELFTH` (`Vision`, `Curator`) from P4-03 UAT-1; `PROFILES_DETAIL_LAUNCH` (`talaria --profile fixture`), `PROFILES_DETAIL_SKILLS` (`Skills:`) from P4-06 UAT-1; `DOCTOR_RUNNING` (`running: `), `DOCTOR_BUILT_FOR` (`built for: `), `DOCTOR_RESULT` (`Result:`) from P4-10 UAT-1 and UAT-2; `WRITE_BANNERS` (`Saved `, `Not restarted`, `Gateway restarted`). A literal not in this section is added here by a specification correction in the same pull request, never left inline.

A text that differs from its issue is a FAIL of the step (the issue's REQ-4); the tester names the constant and the owner issue.

### 9.6 Time budget and defaults

Per-step limits (seconds; `--step-scale` multiplies them, never the deadline):

| Step | Limit |
|---|---|
| startup | 75 |
| plain-answer | 60 |
| config-browse | 30 |
| config-invalid-input | 20 |
| config-secret-refused | 20 |
| providers-tabs | 30 |
| providers-key-masked | 30 |
| models-page | 40 |
| models-bad-preset | 30 |
| profiles-list | 30 |
| profiles-bad-names | 20 |
| doctor-overlay | 30 |
| quit | 30 |
| resume | 150 |
| profiles-cli | 30 |
| doctor-cli | 45 |
| no-gateway | 20 |

The limits sum to 690 s (each includes its setup and teardown keys). `startup`, `plain-answer`, `quit` and `resume` are the larger of the value here and P1-13's own; with P1-13's table as written (shell ready 45 s; startup 30; plain-answer 60; quit 15; resume 30 to restore, 60 for the prompt, 15 to quit) that gives 75, 60, 30 and 150, the values above. The default `--deadline-secs` for `phase4a` is 900 and is fixed: it is not recomputed from the sum. If P1-13's table changes so that the sum passes 750 s (less than 150 s left for the model's latency), the developer does not raise the deadline: it is a stop condition, reported to the operator on PH4A as section 10.3 says. The phase 1 default stays 300. The first live run records the measured times of every step in the journal; a step that needs more than its limit on a healthy machine is a stop condition of the issue.

### 9.7 The command steps

Steps 15 to 17 run in the first pane's shell after `resume` (Talaria has quit in both panes), using the same `Terminal` trait as the first steps (`run`, `read`, `foreground`). They run the binary under test with `HERMES_HOME` unset (steps 15 and 16) or pointing at the empty folder (step 17). `jq` is used only to parse the JSON in steps 15 and 16 and is checked by the preflight (9.2). The suite never passes `--restart` and never writes into `HERMES_HOME`.

### 9.8 Screens and tool output to record

The test author records, from the `main` binary in a throwaway Herdr tab on the live gateway with `--profile fixture`, one screen per read named in 9.3, including each R0 and the screens after each teardown key (about 45 files under `tools/smoke/tests/fixtures/screens/phase4a/`), each scrubbed to `<home>`, `<tmp>` and `<profile>` shapes by the P1-13 scrubber, plus the output of the three command steps. While recording, the author never presses Enter in a key card. Recordings are fixtures, never evidence. If a screen cannot be recorded because a child is not yet on `main`, the author hand-builds it from the source scenario's Then line, marks the file `synthetic` in its first line and says so in the pull request; the live run replaces it. A hand-built screen is the one exception to "never from imagination" and only where this paragraph says so (the issue's test-first rule names it too). The recording also confirms two key facts and the pull request reports them: that `5` then `left` leaves focus on the Mixture of Agents preset list (step 9), and that `herdr pane send-text` of the junk key reaches the key card (step 7).

### 9.9 Tests in detail

All under `tools/smoke/tests/`, written in this order (the issue's table).

1. `screen4a.rs`, 14 functions: `settings_title_found_with_profile`, `settings_category_row_found`, `setting_row_parsed_with_value_and_flags` (includes `delegation.api_key` as `(not set)`, as bullets, and as plain text, which the helper reports as plain), `providers_row_parsed_with_status_and_key_tail`, `providers_tabs_found`, `models_sections_found_in_order`, `models_failed_section_detected`, `profiles_header_and_rows_parsed`, `profiles_refusals_found_in_order` (with the three full texts of 9.5), `write_banner_detected`, `refusal_text_matched_exactly`, `doctor_rows_counted`, `doctor_json_format_and_count_parsed`, `doctor_summary_counts_parsed`.
2. `steps4a.rs`: for each of the 17 steps two functions, `<step>_passes_on_the_recorded_screen` and `<step>_fails_with_its_exact_reason` (34 in all; the steps in the order of 9.3, step names in snake case), each on `FakeTerminal` with `ManualClock`. The pass test feeds the recorded screens from R0 to the idle screen after teardown and asserts the keys sent (equal to the step's row of 9.3) and the elapsed time. The fail test loops over a table with one row per reason of that step in 9.4 (one changed recording each, including, for `quit` and `resume`, token A missing and token A printed more than nA times, and, for every screen step, an overlay left open at R0 and an overlay that survives 3 Esc), and asserts each reason character for character; `providers_tabs` also asserts the SKIP reason. Then `a_full_run_returns_to_idle_between_steps` (a full recorded run: at every step boundary the fake screen is the idle screen, and the keys between two steps are exactly the teardown of the first and the setup of the second), `a_failing_step_stops_the_run_and_cleans_up`, `deadline_names_the_step`, `a_skip_is_never_a_pass`, `live_require_turns_a_skip_into_a_failure` (both on the `providers-tabs` skip), `step_scale_leaves_quiet_periods`.
3. `typed_input4a.rs`: `nothing_typed_contains_unattended`, `slash_lines_are_the_allowed_list`, `no_step_passes_restart_or_confirms_a_review`, `junk_key_is_the_only_secret_typed`, `no_enter_is_sent_while_a_key_card_is_open` (from the key card's opening `enter` to the next `esc`, the only inputs are the junk key's text), `no_line_runs_hermes_update_or_config_set`. They scan every text and key the fake terminal received in a full run and compare the whole sequence with 9.3.
4. `write_banner4a.rs`: `a_saved_banner_fails_every_step`, `a_restart_result_fails_every_step` (inject each banner, including `Saved the key for OpenRouter.`, into each screen step's read and assert the write-banner reason).
5. `cleanup4a.rs`: `tab_closed_after_pass`, `tab_closed_after_fail`, `tab_closed_on_signal`, `empty_home_removed_after_no_gateway_step`, `scratch_and_empty_home_removed_on_every_path`.
6. `cli4a.rs`, on the binary with `fake-herdr`: `help_lists_phase4a_suite`, `phase4a_refuses_other_profile_model_or_provider` (each of `--profile other`, `--model other`, `--provider other` exits 2 with the exact sentence of 9.2 and creates nothing; the three default values given explicitly are accepted), `missing_jq_is_exit_2_and_creates_nothing` (asserts the jq sentence of 9.2, not only the exit code), `deadline_default_is_900_for_phase4a_and_300_for_phase1`, `exit_codes_0_1_2_3`, `failure_report_names_the_pane_and_is_scrubbed` (the fake pane capture holds a home path, the login name, `not-a-real-key` and the junk key; the printed report and the report file hold `<home>`, `<user>` and `<typed-secret>` in their place and none of the four strings).
7. `fixtures_public_safe4a.rs`: `phase4a_fixtures_hold_no_paths_or_names` (no absolute path, no home directory, no login name, no `--profile` value other than `default`, `fixture` and the deliberately unknown names `nosuchprofile` and `nosuch`, which name no profile, and no secret-shaped text other than the two typed ones of the issue's REQ-16 in the recordings of steps 5 and 7).
8. `live_smoke4a.rs`: `live_phase4a_smoke_in_herdr` (`#[ignore]`): runs the suite for real; exit 0 and a line matching `^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$`; fails, never skips, outside Herdr.

**The red run.** The stub commit adds `Suite::Phase4a`, `run_smoke_4a` and the `screen4a.rs` helpers with bodies that panic `not implemented`, and leaves `FromStr`, the help text, the deadline default and the preflight unchanged. So on the red run the `cli4a` tests fail by the old behaviour (`phase4a` refused as an unknown suite, whose text is not the jq or override sentence), every other new test fails by the panic, and a panic of a stub counts as failing for the reason the test names. The one expected pass is `fixtures_public_safe4a::phase4a_fixtures_hold_no_paths_or_names`: it scans already-safe recordings and calls no stub, and making it fail artificially (a private fixture, a planted path) is forbidden. The red-run section of the pull request lists it as the expected pass; any other test that passes on the stubs goes back to the test author.

### 9.10 Cleanup, signals, safety

As P1-13 REQ-12 to REQ-15 (tab closed on every path including SIGINT and SIGTERM, scrubbed reports, no path in any output), plus the empty folder of step 17. The scrubber gains two literal replacements for this suite: `not-a-real-key` and the junk key of P4-02 UAT-2 are each written `<typed-secret>` in every line the suite prints and in every failure report and pane capture it writes. The checks themselves read the unscrubbed pane text (step 7 must see that `sk-or` is absent), and only output is scrubbed. The suite makes no network call itself; the only traffic is Talaria's own to the gateway. No step reaches the provider's key check, because no step presses Enter in a key card.

### 9.11 README additions

`tools/smoke/README.md` gains a section for the phase 4a suite: what it does, that it never writes, the preconditions (a gateway at the pinned release, `fixture` served with its key, no OpenRouter key, `jq`), the exit codes and how to read a FAIL.

### 9.12 Coverage map: which feature is checked where

| Feature | Smoke step | Scenario rows | Cross-feature |
|---|---|---|---|
| Configuration surface (P4-00) | none (probe) | P4-00 UAT-1 to UAT-6 | OP-1 |
| Settings screen (P4-01) | 3, 4, 5 | P4-01 UAT-1 to UAT-6 | XF-2, XF-3, XF-4, XF-6, XF-7, XF-8, XF-9, XF-10 |
| Providers and keys (P4-02) | 6, 7 | P4-02 UAT-1 to UAT-6 | XF-3, XF-4, XF-6, XF-7, XF-8, XF-9, XF-10 |
| Model roles (P4-03) | 8, 9 | P4-03 UAT-1 to UAT-6 | XF-2, XF-3, XF-6, XF-7, XF-8, XF-9, XF-10 |
| Profiles (P4-06) | 10, 11, 15, 17 | P4-06 UAT-1 to UAT-6 | XF-1, XF-2, XF-3, XF-4, XF-5, XF-7, XF-8, XF-9, XF-10 |
| Doctor (P4-10) | 12, 16, 17 | P4-10 UAT-1 to UAT-6 | XF-1, XF-3, XF-4, XF-5, XF-6, XF-7, XF-8, XF-9, XF-10 |

### 9.13 Journal entries

The first live run of the new suite writes a dated entry in `docs/engineering-journal/LEARNINGS.md`: the measured step times, whether the key card accepted `send-text`, how many teardown Esc presses each screen needed, and which read source each screen step needed (the lagging-read lesson of P1-13). A second entry records the rule this unit's repair taught: a validate call that can answer `ok: true` on a rate limit turns a "rejected key" check into a possible write, so a read-only suite never submits a key.

## 10. Questions the operator owns, and facts not verified

### 10.1 The shape of the record

This unit takes the shape the earlier gate was accepted with. Shape A: the four files `phase-4a.md`, `phase-4a-evidence.md`, `check-phase-4a.sh` and `check-phase-4a.test.sh` with the checker of section 6. Shape B: the same 60-row table and evidence in the pull request body and its comments, with no checker. A `phase-3.md` in the merged pull request of PH3-ACC means A; otherwise, and with no ruling anywhere, B. ADR-0012 ([verification is proportionate](../../../../platform-specs/04-architecture/adrs/0012-verification-is-proportionate.md)) decision 4 bars "per-release receipt schemas, digest-bound evidence trees or release-record tests"; shape B honours it most plainly, shape A is a small script and a table. The operator may rule differently on PH4A before the freeze.

Under shape B the lead removes from the issue, before the freeze and in one commit: the checker and its tests (REQ-9), UAT-1, UAT-2, AC-1's second half, AC-4, AC-9 and AC-12; AC-14 then counts 3 scenarios; the other criteria read `$REC` and `$EVI` and run unchanged, with both set to `$X/body.md`, the saved pull request body and evidence comments. AC-5 reads only the lines before the table (`sed -n '1,/^| id |/p' $REC`) and needs a 40-hex `Candidate:` line, so the evidence sections' own `Candidate: <7 hex>. Runner: ...` lines are never counted as header lines. The lead posts the shape on PH4A before stage 0; if PH3-ACC merged with files that match neither shape, stop and ask the operator. Sections 4 to 6 are then reference only.

### 10.2 Decisions, to confirm on the parent issue before the candidate is frozen

1. The shape of the record (10.1).
2. Which operator items are acceptable to run on the operator's own `fixture` profile: a key replaced with itself, a main model changed and restored, a fallback chain reordered and restored, a profile created and deleted, the `fixture` service stopped and started, the gateway restarted. A refusal of any removes its row and step by 8.1 item 4.
3. Whether the operator wants P4-02 UAT-5 (sign in and out of a provider account) in the sitting; it needs a provider account they hold. Dropping it follows 8.1 item 4.

### 10.3 The 900 s deadline and the idle limits

The deadline of 900 s (the sum of the step limits is 690 s) and the XF-9 limits (40960 KiB at start, 102400 KiB after, growth 4096 KiB, CPU 1 s per two minutes; the first two are the architecture's budgets, the other two are sanity checks) are confirmed by the operator in a comment on PH4A before the freeze. Silence means they stand; a rejection is a stop condition of the issue.

### 10.4 Facts not verified, to check at build time and record

- Whether the phase 2a suite's `palette` step still passes with five more local commands in the palette. PH2A-ACC's text says the check is a subset check ("if a later unit adds a local command the check is a subset check, so it still holds"); AUTO-9 is where that is proven. If it fails, the failure is a defect of the phase 2a suite's wording or of a phase 4a child and goes to its owner; this unit edits neither.
- Whether the key card of P4-02 accepts text sent with `herdr pane send-text` (step 7); the paste in the source scenario is described as a paste. If it needs a bracketed paste, step 7 is dropped (8.1 item 4) or the lead asks for `tmux` there. Pressing Enter in the card is never the fix.
- That `snap` is stable between two runs a minute apart with nothing changed (no top-level `*.yaml`, `*.md` or `.env` file of `fixture` is rewritten by the gateway at rest). Check it in stage 0; if a file changes at rest, name it on the issue and leave it out of `snap` by name, never the configuration or key file.
- That `5` then `left` leaves the Mixture of Agents preset list focused (step 9), and that the built binary shows the review title and unsaved-changes question exactly as P4-03's specification writes them (section 9.5).
- That the `default` profile is served and has a model, so XF-6 can open its screens; and that `fixture` has a model of `minimax-m3.1-flash` on `ollama-cloud` (XF-2).
- That the screens' title text for the `default` profile follows the pattern of the source scenarios with the name changed (`Settings · profile default`).
- The title of the model roles page uses a hyphen (`Model roles - profile fixture`) and the settings and providers titles use a middle dot; both are copied from the source scenarios and are the constants to check.
- The restart route is verified at v0.22.4 (section 2.2); recheck it only if `crates/hermes-contract/PINNED` names another release.
- The real wording of every constant in 9.5 on the built binary.

## Appendix A: the verbatim scenarios of the accepted issues

Extracted from the final bodies of the six issues by script on the day this unit was written. `{{id}}` references are written as plain ids. The source issue's own text wins.

### A.P4-00: scenarios of P4-00

#### UAT-1: Read probe against the running gateway
- Persona: builder
- Covers: Success Criteria 1 and 4
- Runner: agent tester in Herdr
- Preconditions: the operator's gateway runs the pinned Hermes release; the `fixture` profile is served (operator sets up).
- Given a Herdr pane in a clean clone of the pull request's branch
- When the tester runs `TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_reads_on_fixture`
- Then the last result line is `LIVE_CONFIG_SURFACE_RESULT: PASSED`, there is one `ROW` line for each of its 43 rows with an outcome and a latency, and no line contains a ticket or a path
- Evidence: the pane text from `herdr pane read <pane> --source recent-unwrapped`; the exit code; the Hermes tag printed by the probe
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: Negative case, no gateway
- Persona: builder
- Covers: Success Criteria 4
- Runner: agent tester in Herdr
- Preconditions: `HERMES_HOME` (read by `default_home()`) points to an empty temporary directory; the gateway is not touched.
- Given the same branch
- When the tester runs `HERMES_HOME="$(mktemp -d)" cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_reads_on_fixture`, then again with `TALARIA_LIVE_REQUIRE=1` set
- Then the first run prints `LIVE_CONFIG_SURFACE_RESULT: SKIPPED (` followed by a plain reason and exits 0; the second run fails with that reason
- Evidence: pane text of both runs and both exit codes
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: Ticket rules are probed and reported
- Persona: builder
- Covers: Key Question 6
- Runner: agent tester in Herdr
- Preconditions: as UAT-1
- Given the same branch
- When the tester runs `TALARIA_LIVE_REQUIRE=1 cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_ticket_rules`
- Then the output has one `ROW` line for each of C16 and C21 to C24 with its observed status, each marked `as-predicted` or `REFUTED`, and the last line is `LIVE_CONFIG_SURFACE_RESULT: PASSED`; every `REFUTED` line is reported in the finding
- Evidence: pane text; exit code
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: Write round trip on the throwaway profile
- Persona: configurer
- Covers: Key Questions 2 and 5
- Runner: operator (it writes to a Hermes profile)
- Preconditions: as UAT-1; the operator has set `display.skin` and `display.show_cost` explicitly in the `fixture` profile's configuration and decided to allow write probes on `fixture`.
- Given the same branch and the operator's own terminal
- When the operator runs `TALARIA_LIVE_REQUIRE=1 TALARIA_PROBE_WRITES=1 cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_writes_round_trip_on_fixture`, then opens the `fixture` profile's configuration file
- Then the probe prints `write-restored` for C10 and C19 and `PASSED`, and the file holds the same settings as before (the operator notes whether its comments survived)
- Evidence: pane text; the operator's written note on the file
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: Writes are refused without the opt-in
- Persona: builder
- Covers: Settled decision 2
- Runner: agent tester in Herdr
- Preconditions: as UAT-1; `TALARIA_PROBE_WRITES` is not set
- Given the same branch
- When the tester runs `cargo test -p hermes-client --test live_config_surface -- --ignored --nocapture probe_writes_round_trip_on_fixture`
- Then it prints `LIVE_CONFIG_SURFACE_RESULT: SKIPPED (write opt-in not set)` and `requests sent: 0` (it stops before discovery), and exits 0
- Evidence: pane text; exit code
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: The finding is usable by the next builder
- Persona: builder
- Covers: Success Criteria 3
- Runner: either
- Preconditions: the pull request is open
- Given the finding document and the gated child issues
- When a reader who has not seen the probe reads the per-child section for `P4-01`
- Then they can name, for reading the configuration, writing one setting and loading the schema, the method or route, the ticket purpose and the measured latency
- Evidence: the reader's three answers in a pull request comment
- Result: PASS / FAIL / BLOCKED / NOT RUN

### A.P4-01: scenarios of P4-01
All: operator preconditions above; a scratch git folder under `$TMPDIR`; a 160 by 48 Herdr pane running `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` (release build), read with `herdr pane read <pane> --source visible`. "Filtered to `compact`": `/`, `compact`, Enter, leaving the cursor on `display.compact`.

#### UAT-1: Browse and search every setting
- Persona: configurer
- Covers: AC-1, AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above; no writes.
- Given the composer is empty and no turn runs
- When the tester types `/config` and Enter (timing until the form appears), presses Right twice, types `/` `compact`, Esc, then `/` `api_key`, then Esc twice
- Then within 3 s `Settings · profile fixture` shows, categories starting `general`, `agent`, `terminal`; after `compact` a `display.compact` row shows a boolean, `set` or `default`, and `may need a restart`; after `api_key`, `delegation.api_key` shows `(not set)` or bullets, never plain text; after the last Esc the transcript is back, unchanged
- Evidence: pane text after each step; the time
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: Change a setting through the review and see it read back
- Persona: configurer
- Covers: AC-1, AC-5, AC-8
- Runner: operator
- Preconditions: `display.compact` explicit `false` in `fixture` (operator sets up); `TALARIA_PROBE_WRITES=1`.
- Given the overlay is open, filtered to `compact`
- When the operator presses Enter, `a`, Enter; then Enter, `a`, Enter again (restoring)
- Then the review shows `Review 1 change(s) · profile fixture` and `display.compact` with `false → true` and `may need a restart`; after Enter the banner reads `Saved 1 change(s) for profile fixture.` and `May need a gateway restart to take effect: display.compact.`, and the row shows `true` and `set`; the restore gives the same banner and `false`
- Evidence: pane text of the review and both banners
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: Set a value from the composer
- Persona: developer
- Covers: AC-6, AC-8
- Runner: operator
- Preconditions: as UAT-2.
- Given the composer is empty
- When the operator types `/config display.compact=true`, Enter, Enter on the review, then `/config display.compact=false`, Enter, Enter on the review
- Then each review opens directly with one `display.compact` row; after each apply the overlay closes and the transcript gains `Settings saved for profile fixture: display.compact = true.` (then `= false.`)
- Evidence: pane text after each command
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: Negative: a secret on the command line is refused and never saved
- Persona: developer
- Covers: AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above; nothing is sent.
- Given the composer is empty
- When the tester types `/config delegation.api_key=not-a-real-key` and Enter, presses Up, types `/quit` and Enter, starts Talaria again with the same command, presses Ctrl+R and types `delegation`, then runs `grep -c not-a-real-key ~/.talaria/history.jsonl` in another pane
- Then the screen shows `delegation.api_key is a secret; set it in /config, not on the command line. Nothing was sent.` and no overlay; Up does not bring the line back; the search shows `No earlier prompt matches "delegation".`; `grep` prints `0` or reports no such file
- Evidence: pane text after Enter, Up, the search; `grep` output
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: Negative: invalid input is refused in place
- Persona: configurer
- Covers: AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above; no writes.
- Given the overlay is open, filtered (`/`, `resume_exchanges`, Enter), cursor on `display.resume_exchanges`
- When the tester presses Enter, types `abc`, Enter, then Esc, then `a`
- Then after the second Enter the detail line shows `Enter a whole number.`, editor still open; after Esc no `*`; `a` shows `No changes to review.`
- Evidence: pane text after each key
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: Negative: a change made outside Talaria is caught before writing
- Persona: configurer
- Covers: AC-5, AC-8
- Runner: operator
- Preconditions: as UAT-2; a second terminal.
- Given the overlay is open, filtered to `compact`, and Enter has pended `true`
- When the operator runs `hermes --profile fixture config set display.compact true` in the second terminal, presses `a` and Enter in Talaria, then `t`; finally runs `hermes --profile fixture config set display.compact false`
- Then the conflict screen lists `display.compact` with `yours true` and `now true`; after `t` the browse view shows `true` with no `*`, the banner `1 setting(s) changed outside Talaria; showing the new values.`, and no `Saved` banner
- Evidence: pane text of the conflict screen and after `t`; the restore command's output
- Result: PASS / FAIL / BLOCKED / NOT RUN

### A.P4-02: scenarios of P4-02
All: operator preconditions met; release build, `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash --record uat-<n>.rec` in a scratch folder under `$TMPDIR`, pane 120 by 40; reads are `herdr pane read <pane> --source visible`, home folder scrubbed to `~`.

#### UAT-1: see the three tabs and reach them from the model picker
- Persona: configurer
- Covers: AC-1, AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above.
- Given an idle session
- When the tester types `/providers`, Enter, reads; Tab, reads; Tab, reads; Esc; types `/model`, Enter, `openrouter`, presses Down to the OpenRouter row, reads, Enter, reads; Esc twice, reads
- Then read 1 shows `Providers · profile fixture`, `Ollama Cloud` with status `ready ·` and `key …` plus four characters, and `OpenRouter` with `no key`; read 2 shows `not signed in` or `signed in` per row; read 3 shows `+ Add an endpoint`; read 4 shows OpenRouter labelled `set up in /providers`; read 5 shows `API key for OpenRouter` and no picker; the last shows the transcript; no key's first characters anywhere
- The section 3 read-only effort-badge hydration exception applies after `/model`, with identical composer rows and remaining status fields and zero outbound `session.mutate` or `prompt.submit` frames.
- Evidence: the six pane reads; the time from Enter to the Keys list
- Result: NOT RUN

#### UAT-2: a rejected key is not saved (negative)
- Persona: configurer
- Covers: AC-1, AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above; internet access.
- Given the Keys tab open
- When the tester types `openrouter` to filter, reads; presses Enter, pastes `sk-or-v1-` followed by 64 zeros (a dummy key), presses Enter, reads; presses Esc twice, reads; types `/quit`, Enter
- Then the card shows 40 bullets and `…` (never the text), then `That API key was rejected. Double-check it and try again.`; the OpenRouter row is unchanged in the last read
- Evidence: the pane reads; `grep -c sk-or-v1-0000 uat-2.rec` prints 0 and the file exists
- Result: NOT RUN

#### UAT-3: replace a key and see the provider ready
- Persona: configurer
- Covers: AC-1, AC-7, AC-11
- Runner: operator (it writes the `fixture` profile's key file)
- Preconditions: as above; the operator has the `ollama-cloud` key in a password manager.
- Given the Keys tab open
- When the operator selects `Ollama Cloud`, presses Enter, pastes the same key, presses Enter
- Then within 5 s the card closes, `Saved the key for Ollama Cloud.` appears, and the row reads `ready · <n> models · key …<last four>`; a new `talaria --profile fixture` session answers a prompt
- Evidence: pane reads; the operator's note that no configuration file was opened (AC-11's journal entry cites this run)
- Result: NOT RUN

#### UAT-4: cancel a sign-in (negative)
- Persona: configurer
- Covers: AC-2, AC-7
- Runner: either
- Preconditions: as above; a `device_code` provider the `fixture` profile is not signed in to (for example `nous`).
- Given the Accounts tab open
- When the tester selects that provider, presses Enter, reads, presses Esc, presses Ctrl+R, reads
- Then the first read shows `1. Open https://`, `2. Enter the code` and a countdown; after Esc the list returns with `Sign-in cancelled.` and the row still reads `not signed in`; nobody opened the link
- Evidence: both pane reads
- Result: NOT RUN

#### UAT-5: sign in and sign out
- Persona: configurer
- Covers: AC-2, AC-7
- Runner: operator (it uses the operator's provider account)
- Preconditions: as UAT-4; the operator chooses a provider whose account they hold.
- Given the Accounts tab open
- When the operator presses Enter, presses `o` (or opens the link by hand), approves in the browser, waits; then presses Ctrl+D, `y`
- Then the card turns into `Signed in to <name>.` within two poll intervals of approval and the row reads `signed in`; after Ctrl+D, `y` it reads `not signed in` with `Signed out of <name>.`
- Evidence: pane reads at each step (account email and any file path in `source_label` scrubbed); the time from approval to `Signed in`
- Result: NOT RUN

#### UAT-6: add, test and delete a custom endpoint, including an unreachable one
- Persona: configurer
- Covers: AC-3, AC-7
- Runner: operator, or the agent tester with the operator's written go-ahead (it writes `fixture`)
- Preconditions: as above; a second pane runs `python3 -m http.server 8799 --bind 127.0.0.1` in a temporary folder holding only `v1/models` = `{"data":[{"id":"stub-model"}]}`; no endpoint `uat-unreachable` or `uat-stub` exists (else stop).
- Given the Endpoints tab open
- When the tester adds `uat-unreachable` with `http://127.0.0.1:9/v1`, model `x`, presses Ctrl+T, reads, presses Esc and `y`; adds `uat-stub` with `http://127.0.0.1:8799/v1` and an empty Model, presses Ctrl+T, reads, Ctrl+S, reads; selects `uat-stub`, presses Ctrl+D, `y`, reads
- Then the first test shows `Could not reach http://127.0.0.1:9/v1`; the second fills Model with `stub-model` and warns `POST /chat/completions returned HTTP 501`; Save shows `Saved uat-stub.` and a row; after delete the list equals the starting list
- Evidence: pane reads; the endpoint list before and after
- Result: NOT RUN

### A.P4-03: scenarios of P4-03
Common preconditions: the operator preconditions; `talaria` from the candidate in a 160x48 Herdr pane on `fixture`. UAT-2 to UAT-4 write: they need the operator's written opt-in, and the tester restores every value.

#### UAT-1: See every model role of a profile
- Persona: configurer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: common
- Given the composer is showing
- When the tester types `/models`, Enter, then presses `2`, `3`, `4`, `5`
- Then the title reads `Model roles - profile fixture`; Main shows a model; Helpers lists `Vision` first and `Curator` twelfth; Subagents shows a route and chain mode; Mixture of Agents shows a preset or the notice; no section says `Failed`
- Evidence: `herdr pane read` text of each section
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: Change the profile's main model and put it back
- Persona: configurer
- Covers: AC-7, AC-8
- Runner: operator, or agent tester with the opt-in
- Preconditions: common; the tester notes the current main model; no Talaria saved default is set
- Given the Main section is open
- When the tester presses Enter, types `ollama`, Enter, chooses a second ollama-cloud model, Ctrl+S, reads the diff, Enter (and `y` if a cost confirmation appears)
- Then the diff shows `model.provider` and `model.default` before and after; the section shows the new model and `Applies to new sessions.`; a new `talaria` session's status line shows it; the original is restored the same way
- Evidence: pane text of the diff, the result and the new session's status line
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: Add fallback models and reorder the chain
- Persona: configurer
- Covers: AC-7, AC-8
- Runner: operator, or agent tester with the opt-in
- Preconditions: common; the chain recorded with `hermes -p fixture fallback list`
- Given the Fallbacks section is open
- When the tester presses `a`, picks `ollama-cloud` and `minimax-m3.1-flash`, adds a second ollama-cloud model with `a`, presses `K` on it, Ctrl+S, Enter
- Then the diff shows `fallback_providers` before and after; the section and `hermes -p fixture fallback list` show the same new order; the recorded chain is restored
- Evidence: pane text before and after; both command outputs
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: Pin a helper model, then let it follow the main model again
- Persona: configurer
- Covers: AC-7, AC-8
- Runner: operator, or agent tester with the opt-in
- Preconditions: common; `Title gen` shows `follows main model`
- Given the Helpers section with `Title gen` selected
- When the tester presses Enter, picks `ollama-cloud` and `minimax-m3.1-flash`, `e`, `low`, Ctrl+S, Enter (and `y` if a cost confirmation appears); then `i`, Ctrl+S, Enter
- Then the row first reads `minimax-m3.1-flash (via ollama-cloud)` and `effort: low`, then `follows main model` and `effort: inherited`, and the second diff shows `auxiliary.title_generation.provider` back to `auto`
- Evidence: pane text of both diffs and both results
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: An incomplete preset is refused and nothing is discarded silently (negative)
- Persona: configurer
- Covers: AC-8
- Runner: agent tester in Herdr (nothing is written)
- Preconditions: common
- Given the Mixture of Agents section is open with focus on the preset list
- When the tester presses `a`, types `uat-test`, Enter, presses Right, `a`, Esc (out of the picker), Ctrl+S; then Esc and `d`
- Then Ctrl+S shows `preset 'uat-test' reference 1: provider and model are required` and no diff; Esc asks apply, discard or stay; `d` closes the page; reopened, no `uat-test` preset exists
- Evidence: pane text of the refusal, the question and the reopened section
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: Hermes is not reachable (negative)
- Persona: developer
- Covers: AC-8
- Runner: operator (stops and restarts the gateway)
- Preconditions: common; the operator stops the gateway after Talaria is attached
- Given the composer is showing
- When the tester types `/models`, Enter, waits 20 seconds, presses Ctrl+R
- Then the sections show `Hermes did not answer. Ctrl+R to retry.` or the sign-in failure text; Talaria neither exits nor freezes; Esc closes the page; once the gateway is back, `/models` loads
- Evidence: pane text of the error state and the recovered page
- Result: PASS / FAIL / BLOCKED / NOT RUN

### A.P4-06: scenarios of P4-06
All: a release build in a Herdr pane at 120 by 40, read with `herdr pane read <pane> --source visible`. Filter with `/fixture` before reading the overlay. Scrub evidence: other profile names to `<profile>`, the home folder to `~`, user and host to `<user>`, `<host>`.

#### UAT-1: see which profiles are served
- Persona: configurer
- Covers: AC-1, AC-7
- Runner: agent tester in Herdr
- Preconditions: the operator's gateway runs; `fixture` is served.
- Given `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` is idle with an empty composer
- When the tester reads the pane; types `/profiles`, Enter, `/fixture`, reads; presses Enter, reads; Esc twice, reads; then runs `talaria profiles fixture default` and `talaria profiles --json fixture`
- Then the header starts `Profiles (`; the `fixture` row shows `served` and `minimax-m3.1-flash · ollama-cloud`; the detail shows `talaria --profile fixture` and a `Skills:` line; the read after Esc equals the read before `/profiles`; the command prints a header row and two `served` rows, `fixture` first, exit 0; the JSON has `"state":"served"`
- Evidence: four pane reads; both outputs with exit codes; time from Enter to the list
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: a stopped profile says why and how to fix it
- Persona: configurer
- Covers: AC-2, AC-8
- Runner: operator (it stops and starts a profile's service)
- Preconditions: as UAT-1; no session open on `fixture`.
- Given `talaria profiles fixture` shows `served`
- When the operator runs `hermes -p fixture gateway stop`, waits 11 s (the topology cache), runs `talaria profiles fixture`; opens `talaria` on `default`, `/profiles`, `/fixture`, reads; `R`, `y`, reads; runs `hermes -p fixture gateway start`, waits 30 s, `r`, reads
- Then the row shows `stopped` with `hermes -p fixture gateway start` (`not discovered` with text J means REQ-2.3's VERIFY failed: report it); the rescan prints `Rescan finished:` and the row stays `stopped`; after the start, `served`
- Evidence: the command output; three pane reads; the rescan result line
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: create a profile and watch it become served
- Persona: configurer
- Covers: AC-3, AC-8
- Runner: operator (it creates a profile)
- Preconditions: as UAT-1; the operator deletes the profile afterwards (`hermes profile delete <name>`).
- Given the overlay is open on `default`
- When the operator presses `n`, types a new name, Enter, `scratch`, Enter, Enter, Enter (defaults), reads the review, Enter, then reads every 5 s for 45 s
- Then the review lists the choices and the shell-command line; the notice starts `Created profile`; the row reads `served` within 40 s (it may show `starting` first); the detail's `Skills:` is above zero
- Evidence: pane reads (name scrubbed to `<profile>`); seconds from Enter to `served`
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: bad names are refused before anything is sent (negative)
- Persona: configurer
- Covers: AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given the overlay is open
- When the tester presses `n`, types `Bad Name!`, Enter, Ctrl+U (clears the field), `default`, Enter, Ctrl+U, ` Fixture `, Enter; Esc; then runs `talaria profiles nosuchprofile`
- Then in order: `Use lower-case letters, digits, - or _, ...`; `default is reserved by Hermes. ...`; `A profile named fixture already exists.`; focus stays in Name each time; the list unchanged; `talaria profiles: no profile named nosuchprofile`, exit 2
- Evidence: pane reads after each try; the command output and exit code
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: edit the description and restore it
- Persona: configurer
- Covers: AC-4, AC-8
- Runner: operator (it writes to `fixture`)
- Preconditions: as UAT-1; note `fixture`'s current description.
- Given the overlay is open with `fixture` selected
- When the operator presses `e`, replaces the text with `P4-06 check`, Enter, Enter; presses `r`; then restores the original the same way
- Then the review reads `description: "<old>" -> "P4-06 check"`; `Saved the description of fixture.`; after `r` the detail shows `P4-06 check`; after restoring, the original
- Evidence: pane reads at each step
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: no gateway (negative)
- Persona: scripter
- Covers: AC-1
- Runner: agent tester in Herdr
- Preconditions: none (an empty temporary Hermes home has no gateway).
- Given `H=$(mktemp -d)`
- When the tester runs `HERMES_HOME=$H talaria profiles; echo "exit=$?"`, then `ls -A $H`
- Then the output says no Hermes gateway is running for this home with a `Next step:` line, then `exit=1`; `ls` prints nothing (no gateway was started, nothing was written)
- Evidence: the output with the exit code; the `ls` output
- Result: PASS / FAIL / BLOCKED / NOT RUN

### A.P4-10: scenarios of P4-10
Live rules: `T=<checkout>/target/debug/talaria` (the branch build); profile `fixture`; UAT-5's `say hi` uses `minimax-m3.1-flash` on `ollama-cloud`. Type with `herdr pane send-text`, then `herdr pane send-keys <pane> enter`; read with `herdr pane read <pane> --source visible`. An empty read, or no gateway where one is needed, is BLOCKED (environment). Never send `unattended: true`. Post each result with its commit on the pull request.

#### UAT-1: A healthy setup reports all rows and exits 0
- Persona: developer
- Covers: AC-8, AC-14
- Runner: agent tester in Herdr
- Preconditions: operator sets up the gateway at the pinned tag with `fixture` served; `jq` installed.
- Given a shell in a fresh Herdr pane
- When the tester runs `"$T" doctor --profile fixture; echo "exit=$?"`, then `"$T" doctor --profile fixture | grep -c "$HOME"`, then `"$T" doctor --json --profile fixture | jq -r '.format, (.checks | length)'`
- Then seven rows start with `ok` or `warn` (none `FAIL` or `skip`), a `Result:` line and the `hermes doctor` line follow, and `exit=0` prints; the second command prints `0`; the third prints `talaria-doctor-v1` and `7`
- Evidence: the pane text from `herdr pane read`
- Result: NOT RUN

#### UAT-2: /doctor opens, expands a row and closes cleanly
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: as UAT-1; Talaria running in pane `P2` with `"$T" --profile fixture`.
- Given an idle composer in `P2`
- When the tester types `/doctor` and Enter, waits until a line starting `Doctor:` appears, presses `Down` twice (selecting `Hermes version`) and `Enter`, reads the pane, presses `Esc`, reads again
- Then the first read shows `Doctor: <a> ok, <w> warnings, <f> failures`, seven rows, the key hint line as the last overlay line, and under `Hermes version` the detail lines `running: …` and `built for: v0.22.4 (4d043038)`; the second read shows the composer and no overlay line
- Evidence: both pane reads
- Result: NOT RUN

#### UAT-3: No gateway gives one failure with a fix, and starts nothing
- Persona: developer
- Covers: AC-10
- Runner: agent tester in Herdr
- Preconditions: none (an empty Hermes home).
- Given `H=$(mktemp -d)`
- When the tester runs `HERMES_HOME="$H" "$T" doctor; echo "exit=$?"; ls -A "$H" | wc -l`
- Then `Gateway` starts with `FAIL` and its next line is exactly ``Fix: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.``; the five rows after `Talaria` and `Gateway` start with `skip`; `exit=1`; `wc -l` prints `0`
- Evidence: the pane text
- Result: NOT RUN

#### UAT-4: A cancelled restart changes nothing
- Persona: configurer
- Covers: AC-11
- Runner: agent tester in Herdr
- Preconditions: as UAT-2, plus a second shell pane.
- Given `"$T" doctor --json --profile fixture` run once in the second pane
- When in `P2` the tester opens `/doctor`, presses `R`, reads, presses `Enter`; then in the second pane runs `"$T" doctor --restart --profile fixture < /dev/null; echo "exit=$?"` and the JSON command again
- Then the read shows `Restart the Hermes gateway?`, a line starting `It serves`, a line with `will be interrupted` or `Nothing is running right now.`, and `[ Cancel ]  [ Restart ]`; Enter shows `Not restarted.` above the footer; the command prints `Restart: Refusing to restart without confirmation: run in a terminal or add --yes.` and `exit=3`; both JSON lines have the same `gateway` summary
- Evidence: the pane reads and both JSON lines
- Result: NOT RUN

#### UAT-5: A confirmed restart comes back and Talaria reconnects
- Persona: configurer
- Covers: AC-12
- Runner: operator
- Preconditions: operator sets up as UAT-1 and accepts that every running turn on every profile is interrupted; Talaria running in a second pane.
- Given no agent work the operator cares about is running
- When the operator runs `"$T" doctor --restart --profile fixture; echo "exit=$?"`, types `yes`, then submits `say hi` in the second pane
- Then the doctor prints the report, then `Restart: Gateway restarted in <n> s (instance <a> → <b>).` with two different prefixes, then a second report with no `FAIL` row, then `exit=0`; the second pane shows an assistant reply to `say hi` without Talaria being restarted
- Evidence: both panes' text and the seconds reported
- Result: NOT RUN

#### UAT-6: An unknown profile and a blank profile are refused
- Persona: developer
- Covers: AC-13
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given a shell pane
- When the tester runs `"$T" doctor --profile nosuch; echo "exit=$?"`, then `"$T" doctor --profile ""; echo "exit=$?"`
- Then the first shows `Profiles` as `FAIL` with `the gateway never discovered a profile named nosuch` and the fix `Create the profile, or check the name with hermes profile list.`, `Attach` as `skip`, and `exit=1`; the second prints a usage error and `exit=2`
- Evidence: the pane text
- Result: NOT RUN

## Settled before the next phase

The parent [settled decisions and operating rulings](https://github.com/infiquetra/talaria/issues/216#issuecomment-6432512594) stand: C-1 supplies shared REST POST and DELETE helpers; C-2 keeps model-role and provider ownership outside the generic settings editor; C-3 keeps `SETTINGS_LINKS` with settings and the profiles premise with profiles; C-4 shares ticketless status reads with an eight-second timeout; C-5 makes all six source issues start gates; C-6 gives this acceptance issue's operator assignments precedence; C-7 accepts the queued client-settings gap; C-8 sets the configuration document limit strictly below 600 lines and links the model-role guide.

The same parent comment records D-1's verified loopback REST surface with read-back and digest checks; D-2's category layout, ownership split and secret controls; D-3's named tester's isolated fixture-only write authorization with restoration (default and restart remain operator-only); D-4's verified provider routes and masked secrets; D-5's atomic model-role blocks; D-6's read-only profile listing with creation and deletion deferred; D-7's ticketless doctor status, operator-confirmed restart and report format; and D-8's accepted client-settings gap. Benchmark dispatch and merge authority were granted there. These historical rulings do not change this gate's operator-only write rows.

The parent [planning and record-shape confirmation](https://github.com/infiquetra/talaria/issues/216#issuecomment-6432751236) records the verified source gates, transport symbols, pinned Hermes contract and document limit. Both Shape A's four committed acceptance files and Shape B's PR body and evidence comments are required. The parent [freeze](https://github.com/infiquetra/talaria/issues/216#issuecomment-6442756797) fixes candidate `8b2fe42b1a93a412ca81536cd2ec0ddcd0346977`, baseline `02126748f9e637a4d6562355cb27447c4a1a3c6a`, Hermes `v0.22.4` and Herdr `herdr 0.9.4` after smoke PR #353.

On 2026-10-27, all 41 agent rows passed on that candidate. Their complete ordered evidence, including earlier failed runs and approved retries, is in [the evidence file](../../../acceptance/v0.7.0/phase-4a-evidence.md). The [record](../../../acceptance/v0.7.0/phase-4a.md) leaves the 19 operator rows NOT RUN. This is agent-stage completion; the gate remains unaccepted until those rows pass.

Section 10.4 facts established by the supplied candidate evidence:

- AUTO-9's `palette` step passes with the additional local commands. AUTO-12 and AUTO-13 each pass all 17 steps in their final runs, including masked text entry without submitting the key card and the incomplete-preset path after `5` then Left.
- P4-03 UAT-5 captures the incomplete-preset refusal and wrapped Mixture of Agents unsaved-changes question; no review is offered for the invalid preset. The review-title template is also present in the candidate's domain view code, while a normal write review remains covered by the pending operator rows.
- XF-2's approved final run captures `minimax-m3.1-flash` on `ollama-cloud` consistently in settings, model roles and profiles. XF-6 captures served `default`, profile-specific titles for both profiles, and no opposite-profile title leakage. The model-role title uses a hyphen; settings and providers use a middle dot.
- The candidate pins Hermes `v0.22.4` at `4d04303818b61bf3aa09278af48356315515965b`, the release on which the restart route was verified. No restart was performed by an agent for this record; the operator restart rows remain NOT RUN.
- The passing smoke runs and source captures verify the displayed constants along their exercised paths, including wrapped hints and the read-only effort-badge exception documented above. This does not claim that unexercised operator-only screen states have been observed.
- All supplied before/after protected-file comparisons are equal, including the two passing phase 4a smoke runs. The lead's [at-rest receipt](https://github.com/infiquetra/talaria/issues/255#issuecomment-6446478367) separately verifies `config.yaml`, `profile.yaml`, `SOUL.md` and `.env` across an 86-second idle interval, from `2026-10-27T15:42:49Z` to `2026-10-27T15:44:15Z`, with the gateway running: four files, `cmp=0`. No file is excluded from the comparison.
