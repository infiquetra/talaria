# SHIP-ACC: Operator acceptance run for v0.9.0, including one issue built through a Talaria seat

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-7, UAT-1 through UAT-3, and the acceptance checks. This page maps the code those rows stand next to, at commit ee5c4881. The sections the issue names copy closed text word for word. It does not restate any other requirement or test.

## What is already in the tree

`docs/acceptance/v0.9.0/` is not in this tree. Tracked acceptance files are only under `docs/acceptance/v0.7.0/`. The column layout is `docs/acceptance/v0.7.0/phase-1.md:9`. `PROOF-4` adds `docs/acceptance/v0.9.0/ship.md` with the first line `# ship acceptance record`, the five header lines `Candidate:`, `Hermes:`, `Herdr:`, `OS:` and `Date:` with nothing after the colon, and a table with the columns `id`, `persona`, `result`, `evidence`, `commit` and no rows. That file is on `main` before this issue starts. REQ-1 fills it.

`CREW` has no acceptance unit, so the three `CREW-2` rows (crew-1, crew-2, crew-3) are in this record. `PROOF` has no acceptance unit. `SHIP-1`, `SHIP-2`, `CREW-1`, `CREW-3` and `REL-090` are not dependencies of this issue.

`docs/dogfood.md` is not in this tree. `SHIP-2` creates it and is not a dependency of this issue. Its section "The scenario that proves the release" says, word for word, what settled decision 2 of the issue says: a Talaria seat takes one issue from its first prompt to a merged pull request. The dogfood row runs that scenario from a git worktree of the candidate commit at `/tmp/talaria-<commit>/dogfood`, on the branch `ship-acc-dogfood-<commit>`, so the seat's working folder is outside the home folder and the checkout. (The v0.9.1 `DOGFOOD` parent runs the same scenario sentence on its own candidate, under its own rules.)

`tools/prepush.sh` is not in this tree. `SEAT-0` provides it.

`tools/release/rc.sh` is in this tree. Its usage line is `tools/release/rc.sh:7`. `--only` sets a partial run (`rc.sh:215`). An `--evidence` path inside the repository is refused (`rc.sh:231`). A dirty tree is refused at `rc.sh:262`. A gate is marked `FAIL` when `git status --porcelain` prints anything, or `HEAD` moved, after it ran (`rc.sh:288`); that is why the issue keeps the draft record outside the checkout until the last gate has finished. A passing gate prints `GATE <id> PASS` and then the seconds (`rc.sh:298`). A partial run sets the summary word at `rc.sh:265` and prints `RC_PARTIAL: PASSED` at `rc.sh:355`. `--evidence` keeps only the last 40 lines of each gate's output (`rc.sh:345`), so the `gateway code_version=` line of the live tests may be cut off there; the issue uses the operator's own capture to `/tmp/talaria-<commit>/live.txt`, the same commands `PIN-ACC` REQ-3 uses.

The `live-tests` row of `tools/release/gates.tsv` (line 26) runs the live test targets with `TALARIA_LIVE_REQUIRE=1`, prints their output, and fails when that output contains `_RESULT: SKIPPED`. At this commit no live test prints `gateway code_version=`; the only producer of that text is `crates/hermes-client/examples/attach.rs:112`. `PROOF-1` adds `gateway_version_line` and prints it from every live test, on the success and skip paths. It is on `main` before this issue starts, through `PIN-ACC`.

`crates/talaria-cli/tests/repository.rs:20-21` lists `docs/acceptance/` and excludes only `docs/acceptance/v0.7.0/`. `python_and_node_toolchain_files_are_gone` (`repository.rs:95`) fails if another tracked path under `docs/acceptance/` appears. Filling a file `PROOF-4` already tracks does not add a path. `PROOF-4` also adds a case that asserts the seven acceptance files contain no `v0.22.4`. It does not look for `PASS`.

## The rows and the commands they already have

`crates/talaria-cli/src/args.rs:45` is `--model`, `:47` is `--provider`, `:51` is `--profile` with default `default`, and `:55` is `--record <PATH>`, which the sig-1 launch uses to write `/tmp/talaria-<commit>/frames.jsonl`. `/quit` is the local command at `crates/talaria-core/src/commands/local.rs:134`. The release gates start their own processes; the runner does not retype the launch line for pin-live.

`hc` facts used by REQ-2 are in `docs/handoff/2026-11-11-v0-9/hc-facts.md`. herdr core sets `HC_TERMINAL_ID` and `HC_SCOPE` for every program it runs. When the caller of `hc send` has `HC_TERMINAL_ID` set, the text arrives wrapped in an `<hc from=...>` tag; with both variables removed it arrives as given. `hc wait` always exits 0; only a reply with `"ended":true` means the program left `working`, and a message for the caller ends the wait only when the caller's environment holds `HC_TERMINAL_ID`. `hc read` prints the screen and history as text.

`talaria doctor` prints one summary line per row and no `running:` line. The `running:` detail is printed only by `--json` and by the `/doctor` overlay (`render_text` in `crates/talaria-core/src/doctor.rs` prints a detail only when it starts with `also:`). The doctor's `MIN_HERMES` (`crates/talaria-core/src/doctor.rs:8`) is older than `OLDEST_SUPPORTED` until `SYNC-2` replaces it.

`docs/workflows/staffing.md:5` says a missing file means ten simultaneous agents. `DEFAULT_MAX_IN_FLIGHT` is `1` at `crates/talaria-workflow/src/staffing/table.rs:8`. `CREW-2` is on `main` before this issue starts and may change that constant and that sentence. crew-1, crew-2, and crew-3 read the result on `main`. They do not edit it. N is the integer in the journal entry `Workflow width default`, which `CREW-1` writes. `CREW-1` is not a dependency. Its entry is on `main` because `CREW-2` starts after it.

Watcher commands and outcomes (from `SYNC-ACC`) holds the commands and the four outcome bullets. The commands copy `databaseId` to `DATABASE_ID`, `headSha` to `HEADSHA`, step 5's tag to `TAG`, and the `tag=` value from step 6 to `PINTAG`. Those rules rely on `compare_versions(left, right)` in `tools/contract-gen/contract_gen.py` being greater than zero when `left` is newer (`contract_gen.py:121-122`).

## The scrub filter

The filter in the issue is the contract's acceptance run convention 11. Its first rule replaces the candidate checkout's own path with `<checkout>`, so a cargo line such as `Compiling talaria-core (<checkout>/crates/talaria-core)` passes the check. That is why the operator and the agent use the one checkout. macOS shows `/tmp` under a `private` folder in some output; the second rule writes it back as `/tmp`. The one-character bracket in the patterns keeps the issue text itself free of that folder name. A terminal address by itself may stay in the record; one that names a machine may not.

## Edge cases

| Situation | Where the issue says what to do |
| --- | --- |
| The working tree is dirty before a gate | REQ-2. `rc.sh:262` refuses it. Commit or stash is not this issue's job. Stop. |
| Live output contains `_RESULT: SKIPPED` | pin-no-skip is not `PASS`, and the gate itself fails. |
| Setup for sig-1 and sig-2 (from `SIG-ACC`) names no drop command | REQ-4. That section says there is no sudo row and no drop command. Do not invent a command. Do not stop the run for that absence. |
| No credit notice frame arrived | REQ-3 and AC-2. The count in `frames.jsonl` is 0, so sig-1 is `NOT RUN (the gateway sent no credit notice)`, and the run still passes. A notice received and not shown is `FAIL`. Any other `NOT RUN`, `BLOCKED`, or `FAIL` fails the run. |
| A row's result is `FAIL` or `BLOCKED (reason)` | REQ-1. Write that result and fill the record. AC-2 does not pass. Do not change it to `PASS`. |
| A comment the issue names is missing | REQ-2. The agent asks for it on the issue, naming it, and waits. That wait is expected and is not a failure. |
| The operator has not named the dogfood issue and profile | REQ-6 and REQ-2. Ask and wait. Do not pick an issue or a profile. Do not start from an empty folder. An agent does not merge. |
| The terminal address names a machine | REQ-2. Do not read it or type into it; ask for a local terminal. |
| `rc.test.sh` fails because `ship.md` contains `v0.22.4` | Stop. Do not edit that test. A `PASS` cell is allowed. |

## Watcher commands and outcomes (from `SYNC-ACC`)

Copied word for word from the closed issue. Where that copy says to ask on this issue, ask on SHIP-ACC.

The commands, in order, are:

1. `gh workflow run hermes-sync.yml --repo infiquetra/talaria --ref main`
2. `gh run list --repo infiquetra/talaria --workflow hermes-sync.yml --limit 1 --json databaseId,status,conclusion,url,headSha,createdAt`
3. Repeat step 2 until `"status"` is `completed` and `createdAt` is after the dispatch.
4. `gh run view DATABASE_ID --repo infiquetra/talaria --json jobs`
5. `gh api repos/NousResearch/hermes-agent/releases/latest --jq .tag_name`
6. `git show HEADSHA:crates/hermes-contract/PINNED`
7. `python3 -c 'import sys; sys.path.insert(0,"tools/contract-gen"); from contract_gen import compare_versions; print(compare_versions(sys.argv[1], sys.argv[2]))' TAG PINTAG`
8. `gh pr list --repo infiquetra/talaria --state open --json title,number,url,state`
9. `gh pr list --repo infiquetra/talaria --state merged --json title,number,url,state`
10. `gh issue list --state open --search "in:title \"Hermes <tag>\"" --limit 100 --json title,number`

Copy `databaseId` to `DATABASE_ID`, `headSha` to `HEADSHA`, step 5's tag to `TAG`, and the `tag=` value from step 6 to `PINTAG`.

If step 5 prints `v0.22.4`, stop before these rules. Do not write the record, do not record `stop-not-newer`, and do not treat that run as an early-stop PASS.

`bump-failed` is `yes` only when the jobs JSON (JavaScript Object Notation) has a step named `bump` whose conclusion is `failure`. `tests-failed` is `yes` only when a step named `test` has conclusion `failure`. Otherwise the flag is `no`, including when the step is absent or skipped. A step did not succeed when it is absent, skipped, or has any conclusion other than `success`. The jobs JSON gives names and conclusions. It does not give the command a step runs. Then set `outcome` and `link` from the first matching rule. Each rule below requires step `compare` conclusion `success`. If `compare` did not succeed, no rule matches. Stop. Do not record `stop-already-open`.

- A failure flag is `yes`. `outcome` is `issue`. `link` is `https://github.com/infiquetra/talaria/issues/` plus the `number` of the step 10 row whose title is exactly `Hermes TAG needs a manual sync`. If no row's title is exactly that, stop. Do not write the link. The step named `issue` does not choose the outcome. Do not read its conclusion for these rules.
- Both flags are `no` and step `pull` has conclusion `success`. `outcome` is `pull`. `link` is `https://github.com/infiquetra/talaria/issues/` plus the `number` of the step 10 row whose title is exactly `Hermes TAG is ready to sync`, one space, then `chore/hermes-sync-TAG`. That is the issue the watcher opened and the branch it pushed. The watcher does not open a pull request. The operator's comment includes the raw output of `git ls-remote --exit-code --heads origin chore/hermes-sync-TAG`, run with `; echo "exit status: $?"` on the same line. Exit status 0 means that branch exists. Exit status 2, or any other status, stops this rule: do not write the link. If that issue is not open, stop. Do not write a pull request url. When the `git ls-remote` output is not in the comment yet, ask for it on this issue and wait. That wait is expected and is not a failure.
- Both flags are `no`, step `bump` did not succeed, and step 7 is not greater than zero. `outcome` is `stop-not-newer`. `link` is `none`. The tag is not newer than `PINNED`, so `SYNC-4` does not run `bump`.
- Both flags are `no`, step `bump` did not succeed, and step 7 is greater than zero. `outcome` is `stop-already-open`. An open issue titled `Hermes TAG is ready to sync` or `Hermes TAG needs a manual sync`, or the remote branch `chore/hermes-sync-TAG`, already exists, so `SYNC-4` does not run `bump`. `link` is `https://github.com/infiquetra/talaria/issues/` plus the `number` of the row whose title matches one of those exactly, otherwise `none`.

## Setup for sig-1 and sig-2 (from `SIG-ACC`)

`SIG-ACC` runs both rows in one launch. This issue does not. sig-1, the agent's row, launches with `/tmp/talaria-<commit>/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --record /tmp/talaria-<commit>/frames.jsonl`. sig-2, the operator's row, launches with `/tmp/talaria-<commit>/talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud`. Where the copy says the credit turn is sent as REQ-2 says, that is the sig-1 launch. Where it says the local session REQ-2 launched, that is the sig-2 launch. Where it says to ask on this issue, ask on SHIP-ACC. The rest of this section is copied word for word from the closed issue.

Operator preconditions, stated once: a gateway at or above v0.23.0 that serves `fixture`; `display.credits_notices` left at its default; a sign-in on the local session that the operator sets up.

A credit notice cannot be produced on demand. The one credit turn is one short plain sentence, sent as REQ-2 says. It is not a slash command.

The operator sets up a sign-in on the local session REQ-2 launched. Read at Hermes v0.23.1: `gateway/slash_commands_login.py` publishes the sign-in text as `notification.show` only when the source platform is local (`_publish_local_notice`, lines 40-54). The operator's comment says whether the sign-in session is local. Do not change the platform. Do not infer that it is not local from a missing notice.

This issue has no sudo row. Do not add a command that drops a connection. Do not invent a drop command, do not add a Rust test, and do not commit a password.

## Background approval for act-2 and act-3 (from `ACT-ACC`)

Copied word for word from the closed issue. Where that copy says to ask on this issue, ask on SHIP-ACC. Where it says the REQ-2 background command, that is the background command in the next paragraph. act-2 is the operator's own terminal. act-3 is a Herdr terminal.

A background session adds `--bg` and the prompt the operator names, as the positional argument after the flags: `/tmp/talaria-<commit>/talaria --bg --profile fixture --model minimax-m3.1-flash --provider ollama-cloud` followed by that prompt. This issue does not choose the prompt. If the operator has not named one, ask on this issue and wait. The operator runs the background command. The agent does not. Do not pass `--worktree`.

`UAT-2` and `UAT-3` are the fleet approval card. Operator sets up one background Hermes session that is waiting on one approval, by running the REQ-2 background command with the prompt the operator named. It is not a Herdr agent row, and it is not the foreground session. If Hermes does not ask, stop. Do not change Hermes configuration. In the foreground seat, submit `/fleet`, press `j` or `k` until that session is selected, and press Space. The list still has focus and the approval card is visible. Its last line contains `Tab note · ctrl+c close`. Press `a` with no Shift. The last line contains `Tab note · ctrl+c back to list`. Press Ctrl+C. The list has focus, the card is still visible, and the last line contains `Tab note · ctrl+c close`. Press Ctrl+C again. The fleet view closes. Submit `/fleet`, select the same session, and press Space. The approval card is still there. None of those lines contains `deny and stop`. `UAT-2` is the operator's own terminal, not inside Herdr. `UAT-3` is the same keys inside a Herdr terminal. Each is `PASS` only when every line here held. If `a` opens a reply editor, stop. That row is not answer focus.

## Paste check for act-4 (from `ACT-ACC`)

Copied word for word from the closed issue. Where it says to ask on this issue, ask on SHIP-ACC. In this issue the UAT-1 seat is the act-1 terminal. The own terminal is the operator's terminal, and it is not Herdr. The REQ-3 prompt is `Reply with the single word pong.` The REQ-2 foreground command is this issue's launch line without `--record`.

REQ-5. `UAT-4` is one paste from the UAT-1 seat and one paste from the operator's own terminal. First, in the UAT-1 seat, after its successful `/copy` and before any other `/copy`, the operator pastes into an empty file outside Talaria. After that read, ask on this issue whether the paste is done, and wait for the operator's comment before sending `/copy 9`. Then, in the operator's own terminal, start the REQ-2 command as a new foreground session. Do not reuse the UAT-2 session. Obtain one finished assistant reply with the REQ-3 prompt, submit `/copy`, and paste into a different empty file outside Talaria. Do not paste into the Talaria composer. Each paste passes when the number of characters pasted equals the count in that session's notice `Copy requested: reply <N>, <count> characters.` and the first line pasted is the first line of the reply on screen. `/copy` copies the reply's source with terminal escapes removed (`ACT-3` REQ-5, `strip_terminal_escapes`, counted with `chars()`). That source is not on screen. Fenced code stays. `UAT-4` is `PASS` only when both pastes pass. If either paste does not, the row is `FAIL`. The evidence quotes both pastes, both counts, and the first line of each reply on screen. It does not claim the terminal confirmed the write.

#### UAT-4: One paste in each terminal

- Persona: developer
- Covers: AC-2
- Runner: operator
- Preconditions: operator sets up. The own terminal is not Herdr. The first paste is the UAT-1 seat after `/copy`, before any other `/copy`. Do not paste into the Talaria composer.
- Given UAT-1 has shown `Copy requested: reply 1,`, and the own terminal later has one finished reply from the REQ-3 prompt
- When the operator counts the UAT-1 clipboard with `pbpaste | wc -m` (on Linux `wl-paste -n | wc -m`) and saves it with `pbpaste > <file>`, then submits `/copy` in the own terminal and counts and saves the clipboard the same way into a different file
- Then the number each count prints equals the count in that session's `Copy requested: reply <N>, <count> characters.` notice, and the first line of the file is the first line of the reply on screen
- Evidence: both files, the two character counts, and the first line of each reply on screen, under `## UAT-4`

## What this proof does not run

`PROOF-4` UAT-3 requires the acceptance files to contain no `PASS`. This issue does not re-run it. `REL-090` pushes the tag and is not a dependency. `SHIP-1` records rulings and is not a dependency. `PIN-3` moves the pin to Hermes v0.23.1 before `PIN-ACC`. `OLDEST_SUPPORTED` is `(0, 23, 0)` in `crates/hermes-contract/src/pinned.rs`, added by `PROOF-1`.
