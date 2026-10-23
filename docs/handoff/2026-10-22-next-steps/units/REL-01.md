# REL-01: Release candidate: integrated regression, replay suites and performance budgets on one commit (unit REL-01)

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what the runner must do and which tests to write first. This document holds what a builder would otherwise guess: the gate catalogue with the failure each gate catches and who fixes it, the exact output lines and algorithms of `tools/release/rc.sh`, the document scanner `tools/release/doc-scan.sh`, the verbatim `gates.tsv`, the guard commands and what they match, what `tools/release/README.md` must say, the workflow text, the Hermes facts with evidence at v0.22.4, the test notes, an edge-case table, the closeout template, the runbook for the lead, notes for the six acceptance scenarios, the reasoning behind the shape, and the facts still to settle at the freeze. Sections are numbered so the issue can point at them. Handoff documents are linked relatively: [the roadmap's rules for every unit](../07-roadmap-and-work-units.md#rules-for-every-unit), [performance budgets](../05-architecture.md#performance-budgets), [decisions the operator owns](../08-risks-and-open-questions.md#decisions-the-operator-owns), [non-goals](../01-vision-and-requirements.md#non-goals), [glossary](../01-vision-and-requirements.md#glossary), [ADR 0012](../../../../platform-specs/04-architecture/adrs/0012-verification-is-proportionate.md), [driving Talaria by hand](../research/u13-hands-on-talaria-drive.md).

## 1. What this unit is for, in plain terms

Every phase of the v0.7.0 plan ends with its own acceptance on its own frozen commit. Nothing yet proves that the last commit on `main`, the one that will carry the tag, passes everything together: all tests, all recorded sessions replayed, all performance budgets, the secret scans, the contract checks, the live smoke suites and the live tests against a real Hermes gateway. This unit adds a small runner that does that in one command, plus a rule for what may change between that run and the tag.

It is deliberately not a ceremony. Draft ADR 0012 says to verify proportionately: automated tests, budgets in continuous integration (CI), and one live drive per release, with no receipts, digests or release-record tests. So the runner prints a result and writes two scratch files; the evidence is pasted into a comment on the issue; nothing is committed except the tooling itself.

Three parties use it. The lead freezes the candidate and runs it. The operator allows the one gateway-wide gate, does the live drive and confirms preconditions. [`REL-03`](https://github.com/infiquetra/talaria/issues/284) (version, changelog, tag, release notes) is expected to ask the runner whether the tag commit may differ from the candidate and to run the CI gates again on the tag commit; confirm that against [`REL-03`](https://github.com/infiquetra/talaria/issues/284)'s own body when it exists.

## 2. The gate catalogue

Each gate is a row of `tools/release/gates.tsv` (section 4). "Fix lives in" says which unit's code a failing gate sends the lead to; this unit never fixes those. Classes: `ci` needs network and `gh` but no gateway; `live` needs the gateway and Herdr; `operator` needs the operator's go-ahead because it changes gateway-wide state for a moment.

| id | Class | What it proves | Typical failure | Fix lives in |
|---|---|---|---|---|
| `fmt` | ci | Code is formatted as the workspace says | a late merge without formatting | the offending unit |
| `clippy` | ci | Lints are clean with warnings denied | a new warning from a toolchain bump | the offending unit |
| `tests` | ci | Every unit, replay, snapshot and property test of every crate passes together on this commit | a test that passes alone and fails beside another (shared state, port, temp file) | the test's unit |
| `release-build` | ci | The release profile builds every binary and every example of the workspace (`--bins --examples`; `--examples` alone builds only example targets) | a feature that only compiles in debug | the offending unit |
| `replay-fixtures` | ci | Every recorded session in `fixtures/frames/` still replays through the shared decoder; at least the seven fixtures of [`P1-04`](https://github.com/infiquetra/talaria/issues/201) exist | a contract bump changed a payload and the fixture no longer decodes | [`P1-04`](https://github.com/infiquetra/talaria/issues/201) or the unit that re-recorded |
| `repo-scan` | ci | No secret-shaped string, home directory, login name or host name is in the recorded fixtures | a fixture recorded without redaction | the author of the fixture |
| `doc-scan` | ci | No secret-shaped string, user-directory path, temporary-directory path, URL credentials or local login, home or host name is in any tracked Markdown or text file, at any depth | a document quoting a real path | the author of the document |
| `contract-check` | ci | The vendored Hermes contract, the generated types and the owner list agree with each other and with `PINNED` | someone hand-edited generated code | [`P0-03`](https://github.com/infiquetra/talaria/issues/196) |
| `contract-vendor` | ci | The vendored contract and `LICENSE.hermes` equal the pinned commit's files byte for byte | the vendor file was edited or the pin moved without re-vendoring | [`P0-03`](https://github.com/infiquetra/talaria/issues/196) |
| `contract-pinned-drift` | ci | A fresh generation from the pinned tag's contract builds and passes the binding tests in a temporary copy of the workspace | the pin was edited without regenerating | [`P0-03`](https://github.com/infiquetra/talaria/issues/196) |
| `contract-latest-drift` | ci, informational | What would break if the pin moved to the newest stable Hermes today | a new Hermes release changed a method | recorded in the notes; a re-pin is its own pull request |
| `bench` | ci | The performance budgets hold on the release build, in a pseudo-terminal at 160 by 48, on recorded fixtures | a slower renderer path; a stray write while idle | the code under test ([`P1-11`](https://github.com/infiquetra/talaria/issues/208)'s harness is right) |
| `no-registry-publish` | ci | Nothing can be published to a package registry (decision 3) | a new crate that neither inherits nor sets `publish = false` | one-line edit allowed here (issue REQ-15) |
| `no-unattended-flag` | ci | No Rust source sets Hermes's `unattended` field to true in a literal form | a test helper that copied a sample frame | the offending unit |
| `no-attribution-lines` | ci | No commit since `python-final` carries a co-author trailer or a tool-credit line | an agent or an editor added a trailer | history is not rewritten on `main`: the operator decides |
| `python-era-removed` | ci | The Python client is gone from the tree | a revert brought `pyproject.toml` back | [`P0-02`](https://github.com/infiquetra/talaria/issues/195) |
| `release-tooling-tests` | ci | This unit's own tests pass | a gate id renamed in one place | this unit |
| `smoke-phase1`, `smoke-phase2a`, `smoke-phase2b`, `smoke-phase3`, `smoke-phase4a`, `smoke-phase4b`, `smoke-phase5` | live | The built binary, driven through Herdr against the real gateway on a free model, does what each phase promised | a regression only a real terminal shows | the unit named in the failing step |
| `live-tests` | live | Every `#[ignore]` live test except the pause test passes in one release-profile run with no SKIPPED marker, including [`P1-12`](https://github.com/infiquetra/talaria/issues/209)'s `live_session.rs::launch_to_usable_composer_within_budget` (median of 15 launches at 160 by 48 at most 200 ms) | a precondition missing; a Hermes behaviour change | the unit that owns the test |
| `live-pause` | operator | [`P3-04`](https://github.com/infiquetra/talaria/issues/242)'s `live_subagents.rs::live_pause_blocks_new_spawns` sees the gateway refuse a new spawn while delegation is paused | the pause was not honoured | [`P3-04`](https://github.com/infiquetra/talaria/issues/242) |

The suite list comes from `smoke run --help` at the freeze. As drafted, the acceptance issues give `phase1` and `phase2a` ([`PH2A-ACC`](https://github.com/infiquetra/talaria/issues/229)), `phase2b` ([`PH2-ACC`](https://github.com/infiquetra/talaria/issues/238)), `phase3` ([`PH3-ACC`](https://github.com/infiquetra/talaria/issues/248)), `phase4a` ([`PH4A-ACC`](https://github.com/infiquetra/talaria/issues/255)), `phase4b` ([`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262)) and `phase5` ([`PH5-ACC`](https://github.com/infiquetra/talaria/issues/273)). [`P1-13`](https://github.com/infiquetra/talaria/issues/210) owns the tool; its own option list has no `--suite`, which the acceptance issues add.

## 3. The runner

### 3.1 Shape

`tools/release/rc.sh`: POSIX `sh`, about 200 lines, `set -u` (not `-e`, because failures are collected). Run `shellcheck` on it and on the other two scripts. No `bash` arrays, no `[[ ]]`, no `local` (use function-name prefixes for variables), no `timeout`, `sed -i`, `date -d`, `readlink -f` or `mktemp --`. `date +%s` for seconds, `mktemp -d` for temporary directories, `git rev-parse --show-toplevel` for the root, `pwd -P` to resolve a directory's real path.

Commands: `rc.sh run ...` and `rc.sh allowed-diff <candidate> <tag-commit>`. No arguments, `-h`, `--help` or an unknown command: print the usage (both forms, one line each) and exit 2.

### 3.2 `run`

Options, any order: `--class ci|live|operator|all` (default `all`), `--only <id>` (repeatable), `--gates <file>` (default `tools/release/gates.tsv` relative to the repository root), `--evidence <file>`, `--summary <file>`.

Steps:

1. Resolve the repository root; outside a git repository print `rc.sh: not inside a git repository` and exit 2.
2. Validate options. An unknown option or class: `rc.sh: unknown option <x>` or `rc.sh: unknown class <x>`, exit 2. An `--evidence` or `--summary` path whose directory resolves (with `pwd -P`) to the repository root or below: `rc.sh: refusing to write <evidence|summary> inside the repository`, exit 2. The directory must exist; otherwise `rc.sh: no such directory for <name>`, exit 2.
3. Read the gates file (missing: `rc.sh: gates file not found`, exit 2). Skip blank lines and lines starting with `#`. A line without exactly four tab-separated fields, a `required` other than `yes`/`no`, a `class` other than `ci`/`live`/`operator`, an empty command, a trailing carriage return, or a repeated id: `rc.sh: gates file line <n>: <problem>`, exit 2. An `--only` id not in the file: `rc.sh: unknown gate <id>`, exit 2.
4. Select gates: class filter first (`all` keeps every class), then the `--only` filter; file order is kept. An empty selection (for example `--class live --only fmt`): `rc.sh: no gate selected`, exit 2.
5. Dirty tree: `git status --porcelain` non-empty (this includes untracked files that are not ignored) prints `rc.sh: the working tree is not clean; commit or stash first`, exit 2. All of steps 1 to 5 happen before any gate runs, so a refusal costs nothing.
6. Record `HEAD` (`git rev-parse HEAD`). For each selected gate: record the start (`date +%s`), run `cd "$ROOT" && sh -c "$command"` with stdout and stderr both into a temporary file, record the exit code, compute seconds, then apply the frozen-tree check (3.6).
7. Result per gate: exit 0 is PASS; 77 is SKIP; anything else is FAIL. Print:
   - `GATE <id> PASS <seconds> s`
   - `GATE <id> FAIL <seconds> s (exit <n>)`, or `GATE <id> FAIL <seconds> s (changed the working tree)` (3.6)
   - `GATE <id> SKIP (<reason>)` where the reason is the last non-empty output line, cut to 120 characters, or `no reason given`.
   For a gate with `required` = `no`, append ` (informational)` to the line.
8. After the last gate (or the stop of 3.6), print the result line (3.3), write the evidence and summary files if asked, and exit.

Gate commands run with the caller's environment unchanged (the live preconditions are environment and Herdr state). The runner sets nothing, unsets nothing, and never touches `~/.hermes`. Gate output never reaches the terminal; only the `GATE` and result lines do. Anything a person needs from a gate's output is read in the evidence file.

### 3.3 Result line and exit code

Let `n` be the number of selected required gates, `p` how many of them passed, `f` how many failed, `s` how many skipped, `k` the number of selected informational gates that did not pass (FAIL or SKIP), and `c` the first 7 characters of the recorded `HEAD`. Required gates that never ran because of a stop (3.6) count in `n` and not in `p`, `f` or `s`.

- All required gates passed: `<WORD>: PASSED (<p> of <n> required gates pass, <k> informational gates not passing, commit <c>)`, exit 0.
- Otherwise: `<WORD>: FAILED (<p> of <n> required gates pass, <f> failed, <s> skipped, <k> informational gates not passing, commit <c>)`, exit 1.
- `<WORD>` is `RC_PARTIAL` when any `--only` was given or `--gates` was given at all (a hand-made gates file can never produce release proof, even when it is a copy); otherwise `RC_RESULT` for class `all`, else `RC_RESULT_CI`, `RC_RESULT_LIVE` or `RC_RESULT_OPERATOR`. The word is followed by a colon.

Tests that need `RC_RESULT:` put their temporary gates file at `tools/release/gates.tsv` inside the temporary repository.

### 3.4 Evidence and summary files

- Evidence (`--evidence <file>`): for each selected gate in order, a line `## <id> (<PASS|FAIL|SKIP>)`, then a fenced block (three backticks and `text`) holding the last 40 lines of the gate's output, then a blank line. A gate output with no lines gets the single line `(no output)`.
- Scrub rule, applied to the text before writing, in this order: (1) the repository root, in its given and its `pwd -P` spelling, becomes `<repo>`; (2) `$HOME`, both spellings, becomes `<home>`; (3) the temporary directory (`${TMPDIR:-/tmp}`, both spellings, because on macOS the temporary folder is reached through a symbolic link) becomes `<tmp>`; (4) any remaining path segment pair made of a user-directory root (`Users` or `home`) and the next segment becomes `<home>` (a `sed` expression of the form `s#/[U]sers/[^/[:space:]]*#<home>#g`, and the same for `home`; the bracket keeps the literal out of this document's own scan); (5) a remaining `private` first segment before `var` or `tmp` is dropped, so the macOS spelling of a temporary path matches neither the issue's AC-12 pattern nor a real folder. Longest strings first within each step, so `<repo>` survives when the repository sits under `$HOME`. Replace with `sed` using a delimiter that cannot occur in the value (escape the value's regular-expression characters first). The README records this rule so the closeout's scrub claim is checkable.
- Summary (`--summary <file>`): the line `| gate | required | class | result | seconds |`, the line `|---|---|---|---|---|`, then one row per selected gate (`SKIP` rows show `-` for seconds; gates that never ran show `NOT RUN` and `-`).
- Both are replaced, not appended, and written even when gates fail.

### 3.5 `allowed-diff`

Purpose: the live gates run once, on the candidate. The tag commit may differ from the candidate only in ways that cannot change behaviour or dependencies. This command decides that.

1. Both arguments must resolve to commits (`git rev-parse --verify -q <x>^{commit}`) and the candidate must be an ancestor of the tag commit (`git merge-base --is-ancestor`); otherwise print `rc.sh: <reason>` and exit 2.
2. `git diff --no-renames --name-only <candidate> <tag>` gives the changed paths. Equal commits give no paths: print `ALLOWED_DIFF: OK (0 files)`, exit 0.
3. Allowed without further checks: exactly `CHANGELOG.md`, `README.md`, `llms.txt`, and any path starting with `docs/`.
4. A `Cargo.toml` (root, `crates/*/Cargo.toml` or `tools/*/Cargo.toml`). Take `git diff -U0 <c> <t> -- <file>`. From each hunk header `@@ -a,b +c,d @@` take the line numbers, so every removed line has a line number in the candidate's file and every added line one in the tag's file. Drop the `---` and `+++` header lines. For each changed line, find its table: the last line before it in that side's file (`git show <rev>:<file>`, read with `awk`) that starts with `[`, with spaces and quotes removed. The line is allowed only in one of three forms:
   - a `version = "..."` line whose table is `[package]` or `[workspace.package]`;
   - an inline dependency line containing `path = "` (with or without a space before `path`) whose table is a dependency table: `dependencies`, `dev-dependencies`, `build-dependencies`, `workspace.dependencies`, or any of these under `target.<...>.`;
   - a `version = "..."` line whose table is an expanded dependency table (`[<dependency table>.<name>]`) that has a `path = "` line on the same side.
   Then mask every occurrence of `version = "<anything>"` on each kept line as `version = "X"` (all occurrences, not only the first). The sorted removed lines (sign stripped) must equal the sorted added lines. Any line outside the three forms, or any difference after masking, makes the file not allowed. A registry dependency has no `path`, so it never reaches the masking step, whether written inline, as an expanded table, or in `[workspace.dependencies]`.
5. `Cargo.lock`. Every changed line must be a `version = "..."` line, the numbers of removed and added lines must be equal, and each changed line must sit in a `[[package]]` block (the lines from a `[[package]]` header to the next blank line) that has no `source = ` line on that side. Local workspace and path packages have no `source`; registry and git packages always have one, so a version-only bump of a registry package is rejected. A `checksum`, `source`, `name` or `dependencies` change is not allowed, which is what a dependency bump looks like.
6. Any other path (source, tests, fixtures, tools, workflows, scripts) is not allowed.
7. If any path is not allowed, print `not allowed:` and one `  <path>` line per such path, exit 1. Otherwise print `ALLOWED_DIFF: OK (<k> files)`, exit 0.

Why masking: a bump from `0.7.0-dev` to `0.7.0` changes only the text inside `version = "..."`. Masking makes both sides equal. The README states the three Cargo.toml forms and the lock rule in these words.

### 3.6 Frozen tree after each gate

After each gate the runner runs `git status --porcelain` and `git rev-parse HEAD` again. If the status is non-empty or `HEAD` differs from the recorded value, the gate is FAIL with `(changed the working tree)`, the runner prints `rc.sh: gate <id> changed the working tree; stopping`, runs no further gate, and the result is FAILED (exit 1) whatever the gate's exit code was. Ignored files (for example `target/`) do not count. Every cargo command in `gates.tsv` that resolves dependencies carries `--locked`, so a stale `Cargo.lock` fails the gate instead of being rewritten.

## 4. `tools/release/gates.tsv` and `tools/release/doc-scan.sh`

### 4.1 Verbatim content to start from

Fields are separated by one tab character (shown here as `<TAB>`). The command is shell text run by `sh -c`. Re-verify every command against the code at the freeze (4.3).

```text
# id<TAB>required<TAB>class<TAB>command
fmt<TAB>yes<TAB>ci<TAB>cargo fmt --check
clippy<TAB>yes<TAB>ci<TAB>cargo clippy --locked --all-targets -- -D warnings
tests<TAB>yes<TAB>ci<TAB>cargo test --workspace --locked
release-build<TAB>yes<TAB>ci<TAB>cargo build --release --workspace --locked --bins --examples
replay-fixtures<TAB>yes<TAB>ci<TAB>n=0; for f in fixtures/frames/*.jsonl; do cargo run -q --locked -p record-fixture -- replay "$f" > /dev/null || exit 1; n=$((n+1)); done; test "$n" -ge 7
repo-scan<TAB>yes<TAB>ci<TAB>cargo run -q --locked -p record-fixture -- scan fixtures/frames
doc-scan<TAB>yes<TAB>ci<TAB>sh tools/release/doc-scan.sh
contract-check<TAB>yes<TAB>ci<TAB>command -v cargo-typify > /dev/null 2>&1 || { echo 'cargo-typify is not installed'; exit 77; }; CONTRACT_GEN_REQUIRE_TYPIFY=1 python3 -I tools/contract-gen/contract_gen.py check --require-typify
contract-vendor<TAB>yes<TAB>ci<TAB>python3 -I tools/contract-gen/contract_gen.py verify-vendor
contract-pinned-drift<TAB>yes<TAB>ci<TAB>python3 -I tools/contract-gen/contract_gen.py drift --tag "$(sed -n 's/^tag=//p' crates/hermes-contract/PINNED)"
contract-latest-drift<TAB>no<TAB>ci<TAB>python3 -I tools/contract-gen/contract_gen.py drift --tag latest
bench<TAB>yes<TAB>ci<TAB>cargo build --release --locked -p talaria-cli -p talaria-bench && target/release/talaria-bench run --binary target/release/talaria
no-registry-publish<TAB>yes<TAB>ci<TAB>cargo metadata --locked --no-deps --format-version 1 | python3 -I -c 'import json,sys; bad=[p["name"] for p in json.load(sys.stdin)["packages"] if p["publish"]!=[]]; print("crates without publish = false: "+", ".join(bad)) if bad else None; sys.exit(1 if bad else 0)'
no-unattended-flag<TAB>yes<TAB>ci<TAB>! git grep -nE '"unattended"[[:space:]]*:[[:space:]]*true|unattended[[:space:]]*[:=][[:space:]]*(Some\()?true|\.unattended\([[:space:]]*(Some\()?true' -- 'crates/*/src/**' 'tools/*/src/**' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//' | grep .
no-attribution-lines<TAB>yes<TAB>ci<TAB>git rev-parse -q --verify refs/tags/python-final > /dev/null && ! git log --format=%B python-final..HEAD | grep -iE '^co-authored-b[y]|^generated wit[h]'
python-era-removed<TAB>yes<TAB>ci<TAB>test -z "$(git ls-files pyproject.toml uv.lock package.json 'talaria/*.py')"
release-tooling-tests<TAB>yes<TAB>ci<TAB>sh tools/release/tests/rc.test.sh
smoke-phase1<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase1 --talaria target/release/talaria
smoke-phase2a<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase2a --talaria target/release/talaria
smoke-phase2b<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase2b --talaria target/release/talaria
smoke-phase3<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase3 --talaria target/release/talaria
smoke-phase4a<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase4a --talaria target/release/talaria
smoke-phase4b<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase4b --talaria target/release/talaria
smoke-phase5<TAB>yes<TAB>live<TAB>TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite phase5 --talaria target/release/talaria
live-tests<TAB>yes<TAB>live<TAB>out=$(TALARIA_LIVE_REQUIRE=1 cargo test --workspace --release --locked -- --ignored --nocapture --skip live_pause_blocks_new_spawns 2>&1); s=$?; printf '%s\n' "$out"; test "$s" -eq 0 && ! printf '%s\n' "$out" | grep -q '_RESULT: SKIPPED'
live-pause<TAB>yes<TAB>operator<TAB>out=$(TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_PAUSE=1 cargo test --release --locked -p talaria-app --test live_subagents -- --ignored --nocapture --exact live_pause_blocks_new_spawns 2>&1); s=$?; printf '%s\n' "$out"; test "$s" -eq 0 && printf '%s\n' "$out" | grep -q 'LIVE_SUBAGENT_RESULT: PASSED'
```

Corrections against the first draft of this table, so the next reader knows why the text looks as it does: `release-build` gained `--bins` (with `--examples` alone cargo builds only example targets; checked in a scratch workspace and in `cargo build --help`); `no-unattended-flag` gained `/**` on both pathspecs (a git pathspec glob matches the whole path, so `crates/*/src` matches no file and the guard passed everywhere; checked in a scratch repository, where `crates/*/src` found 0 lines and `crates/*/src/**` found the planted ones); `contract-check` gained the `cargo-typify` check and `CONTRACT_GEN_REQUIRE_TYPIFY=1` (the tool can otherwise skip with exit 0, [`P0-03`](https://github.com/infiquetra/talaria/issues/196) REQ-3); `contract-vendor` is new (`drift` does not compare committed bytes; `verify-vendor` does); `repo-scan` lost its `scan docs` half (the scanner reads only `*.jsonl` directly in the folder, [`P1-04`](https://github.com/infiquetra/talaria/issues/201) spec section 7, so it never read a document) and `doc-scan` replaced it; `live-tests` gained the SKIPPED check and lost the pause test to `live-pause`; every resolving cargo call gained `--locked`.

Notes on single gates:

- `replay-fixtures`: `record-fixture replay <file>` prints `replayed <n> inbound frames; final statuses: <list>` and exits non-zero when decoding fails ([`P1-04`](https://github.com/infiquetra/talaria/issues/201) REQ-17). The count of at least 7 is the seven fixtures of [`P1-04`](https://github.com/infiquetra/talaria/issues/201); later units may add more, and the number never goes down.
- `repo-scan`: `record-fixture scan <dir>` reads the `*.jsonl` files directly inside the directory, not recursively, and prints `<file>:<line> <rule>`; exit 0 clean, 1 on a hit, 2 when a precondition is absent ([`P1-04`](https://github.com/infiquetra/talaria/issues/201) REQ-16, REQ-17). `fixtures/frames/` is flat, so it covers every fixture.
- `contract-check`, `contract-vendor`, `contract-pinned-drift`, `contract-latest-drift`: exit 0 success, 1 expected failure (also unknown tag, no network, `gh` or typify missing), 2 usage mistake ([`P0-03`](https://github.com/infiquetra/talaria/issues/196) REQ-3). `verify-vendor` and `drift` fetch from GitHub with `gh api`, so they need `gh` signed in (in CI, `GH_TOKEN`) and network. `drift` regenerates, builds and binding-tests in a temporary copy and never edits the repository ([`P0-03`](https://github.com/infiquetra/talaria/issues/196) REQ-13). Without `--hermes-checkout` it keeps the committed owner list and says so; that is acceptable here.
- `contract-latest-drift` is informational because the pin deliberately lags newer Hermes releases; a failure there is a line in the release notes.
- `bench` builds the two binaries it needs and runs [`P1-11`](https://github.com/infiquetra/talaria/issues/208)'s command. Exit 0 when no budget row is FAIL, 1 on a FAIL, 2 when a precondition is missing and nothing was measured ([`P1-11`](https://github.com/infiquetra/talaria/issues/208) REQ-1). Its launch-to-composer row is SKIPPED there (it needs a gateway); `live-tests` covers it.
- `no-registry-publish`: see 4.2. `python3 -I` matches the invocation [`P0-03`](https://github.com/infiquetra/talaria/issues/196) uses.
- Smoke gates: `smoke run` exits 2 with one plain sentence when preflight fails (not in a Herdr pane, `talaria` path wrong, for `phase2a` no `tmux`, and for `phase4a`, `phase4b` and `phase5` no `jq`) and prints no `SMOKE_RESULT` line ([`P1-13`](https://github.com/infiquetra/talaria/issues/210) REQ-2). With `TALARIA_LIVE_REQUIRE=1` a skipped step fails the run ([`P1-13`](https://github.com/infiquetra/talaria/issues/210) REQ-12). No `--profile`, `--model` or `--provider` is passed: [`P1-13`](https://github.com/infiquetra/talaria/issues/210)'s defaults (`fixture`, `minimax-m3.1-flash`, `ollama-cloud`) are exactly what the phase 3, 4a, 4b and 5 preflights require ([`PH3-ACC`](https://github.com/infiquetra/talaria/issues/248), [`PH4A-ACC`](https://github.com/infiquetra/talaria/issues/255), [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262), [`PH5-ACC`](https://github.com/infiquetra/talaria/issues/273)), and an override would trip them.
- `live-tests`: `cargo test --workspace --release -- --ignored` runs every ignored test of every crate: [`P1-12`](https://github.com/infiquetra/talaria/issues/209)'s `live_session.rs`, [`P1-13`](https://github.com/infiquetra/talaria/issues/210)'s `live_full_smoke_in_herdr` (fails, never skips, without `HERDR_ENV`), [`P2-14`](https://github.com/infiquetra/talaria/issues/235)'s live print tests, [`P3-04`](https://github.com/infiquetra/talaria/issues/242)'s `live_subagents.rs` tests 49 to 51, and any later ones. With `TALARIA_LIVE_REQUIRE=1` a missing precondition fails a test instead of skipping it. The one documented exception is [`P3-04`](https://github.com/infiquetra/talaria/issues/242)'s test 52 (`live_pause_blocks_new_spawns`), which prints `LIVE_SUBAGENT_RESULT: SKIPPED (TALARIA_LIVE_PAUSE not set)` and passes even under `TALARIA_LIVE_REQUIRE=1` ([`P3-04`](https://github.com/infiquetra/talaria/issues/242) spec 9.5); it is skipped here and has its own gate. The extra `_RESULT: SKIPPED` check fails the gate if any other test has an undocumented skip path that passes. At the freeze, grep every `tests/live_*.rs` for `SKIPPED` and list the markers in the README. If the whole set cannot run in one pass (time, or two tests conflicting over gateway state), split by test name into further gates with `--skip <name>` and an exact filter, never by removing a test.
- `live-pause`: pauses spawning for every session on the gateway while it runs, so it is class `operator`. It runs in the full run only when the operator starts that run, or has posted `RC pause: ok <7 hex>` on the issue for this candidate. It runs by itself; no other live activity runs at the same time ([`PH3-ACC`](https://github.com/infiquetra/talaria/issues/248) REQ-15 runs one live scenario at a time).

### 4.2 Guard commands: what they match

- `no-registry-publish`: `cargo metadata --no-deps` reports a package's `publish` as an empty list when the manifest says `publish = false`, a list of registry names when it names registries, and `null` when the key is absent. Inheritance counts: [`P0-02`](https://github.com/infiquetra/talaria/issues/195) REQ-11 puts `publish = false` in `[workspace.package]` and every member sets `publish.workspace = true`, which metadata also reports as `[]` (checked while repairing this document on a scratch workspace). Only `[]` passes. The snippet prints the offending crate names. Before adding `publish = false` to any crate, run the command and add it only where metadata shows `null`; [`P1-11`](https://github.com/infiquetra/talaria/issues/208)'s `talaria-bench` already sets it literally, and adding it everywhere would leave two spellings.
- `no-unattended-flag`: matches, in tracked files under `crates/*/src/**` and `tools/*/src/**`, a line that sets the field to true in one of five forms: a struct field (`unattended: true`), an optional field (`unattended: Some(true)`), an assignment (`params.unattended = true` or `let unattended = true`), a builder call (`.unattended(true)` or `.unattended(Some(true))`, the form `cargo typify` builders produce), and JSON (`"unattended": true`). Lines whose first non-space characters are `//` are dropped, so a comment such as "never send it" does not fail. It does not match `== true` comparisons, `false` values or absence assertions. Checked while repairing this document: the regular expression matched all five planted forms and none of a comment, `assert!(!text.contains("unattended"))`, `let unattended = false;` or `if params.unattended == true {}`. What it cannot catch: a value from a variable, or a field split across lines (rustfmt keeps a short field and its value on one line). Those are covered structurally by [`P0-03`](https://github.com/infiquetra/talaria/issues/196)'s `check_prompt_submit`, which fails when `unattended` is true ([`P0-03`](https://github.com/infiquetra/talaria/issues/196) REQ-10), and by the outbound-frame tests of the units that send `prompt.submit`. Test directories are outside the pathspec. Hermes's meaning of the field is why the guard exists (H5).
- The phase hygiene scans are stricter in other ways: [`PH1-ACC`](https://github.com/infiquetra/talaria/issues/211)'s AUTO-5 counts comments and also looks for `end_reason` and `config.set`, but only in `crates/talaria-app/src` and `crates/talaria-cli/src`. The two checks are deliberately different; the README says so, and neither replaces the other.
- `no-attribution-lines`: `python-final` is the annotated tag [`P0-02`](https://github.com/infiquetra/talaria/issues/195) creates on the last Python-era commit. The first part of the command fails the gate when the tag is missing (a shallow checkout), so a missing tag can never look like "no trailers". The workflow checks out with `fetch-depth: 0`. The bracketed last letters keep the phrases out of this repository's own text.
- `python-era-removed`: a short list of files that only the Python era had; a smoke alarm, not a full audit. `git ls-files` with a pathspec lists tracked files only.

### 4.3 Finding the real gate list at the freeze

The table in 4.1 is a starting point written before phases 4 and 5 were built. At the freeze the builder:

1. Lists the smoke suites with `cargo run -q --locked -p smoke -- run --help`. The acceptance issues require the suite names to appear in that help; read the exact format and write case 17's extraction for it. One gate per suite, in the order the help lists them. Case 17 enforces both directions.
2. Reads `tools/bench/README.md` for the exact `talaria-bench run` options (whether `--binary` is mandatory, the report option) and for the budget key names in `tools/bench/budgets.toml` (UAT-4 edits the launch-to-first-frame key).
3. Reads `record-fixture --help` for `replay` and `scan`, and `contract_gen.py --help` for `check`, `verify-vendor` and `drift`.
4. Greps the live test files for `SKIPPED` markers (4.1, `live-tests`).
5. Runs `cargo metadata --no-deps --format-version 1` for the `publish` values (4.2).
6. Runs each command once by hand and puts the verified line in the file. A command that cannot run is a stop condition, not a gate to delete.

### 4.4 What `tools/release/README.md` says

Short and factual, in this order:

1. How to run: the two forms, the classes, that the class filter applies before `--only`, that an empty selection is a usage error, that `--only` or `--gates` always gives `RC_PARTIAL:`, and that the evidence and summary files must live outside the worktree.
2. Preconditions: Hermes at the pinned tag with the gateway running; profile `fixture` with `approvals.mode: manual` and `minimax-m3.1-flash` on `ollama-cloud`; a Herdr pane; `tmux`; `jq`; `cargo-typify` 0.8.0; `python3` 3.9 or later; `gh` signed in; network. The `ci` class needs `cargo-typify` (for `contract-check`) and the last three; in CI the workflow token serves `gh`.
3. Exit-code map per gate: smoke 2 is preflight (BLOCKED (environment)), 1 is a failed step; bench 2 is a missing precondition, 1 a budget FAIL; `contract_gen.py` 1 is an expected failure including no network or a missing tool, 2 a usage mistake; `record-fixture` 2 is a missing precondition; the runner's own 77 only from `contract-check`'s typify check. A drift gate that failed for network reasons is BLOCKED (environment), not a product defect.
4. The scrub rule of 3.4.
5. Scan limits: `repo-scan` reads only `*.jsonl` directly in `fixtures/frames`; `doc-scan` reads tracked `*.md` and `*.txt` at any depth with five rules (4.5) and cannot catch a secret shape it has no rule for.
6. Differences from the phase hygiene scans (4.2).
7. Smoke: `smoke run --help` is the authority for the suite list; never pass `--profile`, `--model` or `--provider`.
8. `bench` runs on Linux in `rc-linux` as it already does in [`P1-11`](https://github.com/infiquetra/talaria/issues/208)'s `bench` job; a Linux budget failure is a defect like a macOS one. The release evidence quotes both readings.
9. The `allowed-diff` rules of 3.5 in words.
10. How to check the history early: `sh tools/release/rc.sh run --only no-attribution-lines`.

### 4.5 `tools/release/doc-scan.sh`

`sh tools/release/doc-scan.sh [<root>]`, POSIX `sh`. `<root>` defaults to `git rev-parse --show-toplevel`. Files: `git -C <root> ls-files -- '*.md' '*.txt'` (tracked files at any depth, in name order). Rules, each a `grep -nE` (or `grep -nF` for literals) per file:

| Rule id | Matches |
|---|---|
| `home-path` | a user-directory root followed by a name character: the expression `/(Users\|home)/[A-Za-z0-9._-]` (placeholders such as `<home>` do not match) |
| `tmp-path` | a macOS temporary folder: `/(private/)?var/folders/` |
| `sk-key` | `sk-[A-Za-z0-9_-]{20,}` (same id as [`P1-04`](https://github.com/infiquetra/talaria/issues/201)'s rule) |
| `url-userinfo` | `://[^/[:space:]:@]+:[^/[:space:]@]+@` (same id as [`P1-04`](https://github.com/infiquetra/talaria/issues/201)'s rule) |
| `forbidden-literal` | `$HOME`, `${USER:-$LOGNAME}` and `hostname -s`, each only when set and four or more characters; otherwise one line `note: <home\|login\|host> not checked (unset or under four characters)`, never the value |

Output: one line per hit, `<path>:<line> <rule>`, the path relative to `<root>`, never the matched text (job output on a public repository is public). Exit 1 when any hit exists; otherwise print `DOC_SCAN: OK (<n> files)` and exit 0. Not inside a git repository: one plain line, exit 2. A hit is fixed in the document by its owner; there is no allow-list. This script reuses no code from [`P1-04`](https://github.com/infiquetra/talaria/issues/201) and does not change its command; it shares two rule ids so a reader sees the same names.

## 5. Hermes facts used (verified at v0.22.4 unless marked)

This unit depends on very little of Hermes itself; it drives the tools other units built.

| # | Fact | Evidence | Consequence here |
|---|---|---|---|
| H1 | The contract file's `info.version` is the string `"1"` | `apps/shared/src/gateway-contract.openrpc.json:5` at Hermes v0.22.4 (`jq .info` prints `title`, `version: "1"`, `description`) | The version string cannot reveal drift. The gates compare file contents and regenerate. A meaningful version is an upstream wish in [08](../08-risks-and-open-questions.md#upstream-contributions-worth-proposing-to-hermes), item 4 |
| H2 | The newest stable release was `v0.22.4`, published 2026-10-21T18:49:24Z | `gh api repos/NousResearch/hermes-agent/releases/latest --jq '.tag_name+" "+.published_at'` run on 2026-10-22 | The `Hermes:` line of the closeout. On the day of the freeze, run the command again |
| H3 | `runtime.describe` takes an empty parameter object and returns `instance_id`, `profile_id`, `authority_epoch`, `capabilities`, `session_create`, and an optional `skin` | contract `components.schemas.RuntimeDescribeResult` at line 34422, method entry at line 47747; source `tui_gateway/contracts/canonical.py:341-351` | Not called by this unit. It returns no release tag, so "which Hermes ran" is answered by the pin and the operator's precondition, not by the gateway |
| H4 | Talaria's minimum supported Hermes is v0.22.3 | Handoff [glossary](../01-vision-and-requirements.md#glossary) ("Talaria's minimum supported Hermes is v0.22.3"); [`P0-03`](https://github.com/infiquetra/talaria/issues/196) REQ-1 enforces "v0.22.3 or later, compared numerically" when it chooses the pin. Historical provenance: `gh api 'repos/NousResearch/hermes-agent/contents/gateway/session_authority.py?ref=v0.22.3'` returned blob `fe9edf5be5c4d303573c9073f8012203bade8560`, and the same request at `ref=v0.22.2` returned 404 (checked 2026-10-22), so the session authority file first appears at v0.22.3 | The pin check is [`P0-03`](https://github.com/infiquetra/talaria/issues/196)'s, enforced through `contract-check`; the closeout records the pinned tag. Re-verify only the candidate's pin and the live gateway's identity |
| H5 | `prompt.submit`'s `unattended` flag, when true on a finite turn, makes the whole turn auto-approve | `gateway/session_finite.py:27-40` (`unattended_turn()` reads a context variable that `finite_turn_scope(finite, unattended)` sets only when both are true) and lines 46 to 56 (the flag must be a boolean, true needs `finite: true`, and only the local CLI principal on its own CLI session may ask for it); `tools/approval.py:327-333` (`_yolo_active()` returns `unattended_turn()`) and line 990 (approval is skipped when `_yolo_active()` is true) | The reason for the `no-unattended-flag` guard and for decision 5 of the issue |

## 6. Test notes

### 6.1 Layout

One file, `tools/release/tests/rc.test.sh`, plain `sh`. A helper `case_start <name>` and `case_ok` print `CASE <name>: ok`; a failing assertion prints `CASE <name>: FAILED: <what>` and counts it. Last line: `RC_TESTS: PASSED (23 cases)` or `RC_TESTS: FAILED (<f> failed)`; exit 0 or 1. The count is the number of cases that ran; the acceptance criteria pin it at 23 (10 `run-`, 5 `allowed-diff-`, 3 `gates-`, 4 `gate-`, 1 `workflow-`).

### 6.2 The stubs and the red run

The test author commits first (`test(release): add the runner tests and stubs`): `tools/release/rc.sh` and `tools/release/doc-scan.sh` each printing `RC_RESULT: FAILED (stub)` and exiting 1; `tools/release/gates.tsv` holding one comment line; no `release-candidate.yml` yet. The red run lists all 23 cases failing, each for its own reason (no `GATE` lines, wrong exit code, missing `ALLOWED_DIFF` or `DOC_SCAN` line, missing gate ids, missing workflow). The developer's commits turn them green.

### 6.3 Temporary repositories

Every case that runs a script builds a repository under `mktemp -d`: `git init -q`, `git config user.email test@example.invalid`, `git config user.name test`, one commit. The script under test is the real file, called by absolute path from inside the temporary repository; the gates file sits at `tools/release/gates.tsv` inside it (so `RC_RESULT:` is reachable) except in case 5's `--gates` variant. Assertions use `grep -F` on captured output; none embeds an absolute path. Gate commands in temporary gates files are trivial: `true`, `echo boom; exit 3`, `echo needs gateway; exit 77`, a command that writes a marker file, a command that appends to a tracked file (case 7), a command printing `seq 1 97` and then `$PWD`, `$HOME` and `${TMPDIR:-/tmp}` on lines 98 to 100 (case 9: evidence keeps lines 61 to 100, that is numbers 61 to 97 and the three placeholders, and not 60). Evidence and summary paths in cases point into a second `mktemp -d`, outside the temporary repository.

Case 6 creates a marker file from a gate command and asserts the file does not exist afterwards, proving no command ran. Case 8 includes `--class live --only <a ci id>` (empty selection) and `--evidence <path inside the temporary repository>`.

### 6.4 Cases 11 to 15 (`allowed-diff`)

Each builds two commits in a temporary repository with a root `Cargo.toml` (a `[workspace.package]` with `version`), one member `crates/a/Cargo.toml`, and a hand-written `Cargo.lock` with one local package (no `source`) and one registry package (with `source` and `checksum`).

- 12 allows: the `[workspace.package]` version; an inline `b = { path = "../b", version = "0.7.0-dev" }` version; an expanded `[dependencies.b]` table with `path` and `version`; the local lock package's version.
- 13 rejects, each in its own commit pair: `serde = "1"` to `"2"`; `[dependencies.serde]` `version = "1"` to `"2"` with no `path`; a `[workspace.dependencies]` registry entry; the registry lock package's version only; its `checksum`; a lock diff with two removed and one added version line.

### 6.5 Cases 19 to 22: use the real command text

Extract a command with `awk -F '\t' '$1 == "<id>" { print $4 }' tools/release/gates.tsv` (relative to the real repository root, captured before changing directory) and run it with `sh -c` inside a temporary directory:

- 19: a temporary Cargo workspace with two crates and `[workspace.package] publish = false`; crate one inherits it (`publish.workspace = true`), crate two sets it literally (exit 0); then crate two loses the key (exit 1, its name printed). `cargo metadata --locked --no-deps` works offline for crates with no dependencies and needs no `Cargo.lock` (checked while repairing this document: exit 0 with no lock file; an inherited `publish = false` reported `[]` and an absent key `None`).
- 20: a temporary git repository with `crates/a/src/lib.rs`, files added with `git add` (git grep scans tracked files). Must fail, each alone: `let f = Frame { unattended: Some(true) };`, `params.unattended = true;`, `let p = Params::builder().unattended(true);`, `json!({"unattended": true})`, `    unattended: true,`. Must pass: `// never send it as true here`, `assert!(!text.contains("unattended"));`, `let unattended = false;`.
- 21: a temporary repository: commit one tagged `python-final` (annotated); commit two has the message `feat: x`, a blank line and a co-author trailer for an example address (built at run time from two pieces with `printf`, so the test file never holds the phrase; the gate fails); a variant with only the plain message passes; a repository without the tag fails.
- 22: runs the real `tools/release/doc-scan.sh` against a temporary repository holding `a/b/c.md` whose line 1 is a run-time-built `sk-` key (`printf 'sk-%s\n' "$(printf 'a%.0s' $(seq 1 24))"`) and line 2 a user-directory path built from pieces (`printf '/%s/%s/x\n' Users someone`). Expect exit 1 and the lines `a/b/c.md:1 sk-key` and `a/b/c.md:2 home-path`, and that the output does not contain the planted text. A second repository with a clean `docs/ok.md` prints `DOC_SCAN: OK (1 files)` and exits 0. The case sets `HOME`, `USER` and `LOGNAME` to values that do not occur in the files.

### 6.6 Cases 16 to 18 (the gate file)

- 16 parses the real `gates.tsv` with `awk -F '\t'`: field count, unique ids, `required` and `class` values, the presence of every id the issue's REQ-8 names, and the command shapes of the issue's case table (`--bins --examples`, `CONTRACT_GEN_REQUIRE_TYPIFY=1`, `--locked` on every `cargo build`, `test`, `clippy`, `run` and `metadata`, `--skip live_pause_blocks_new_spawns` in `live-tests`, `TALARIA_LIVE_PAUSE=1` in `live-pause`, `/**` on the `no-unattended-flag` pathspecs).
- 17 runs `cargo run -q --locked -p smoke -- run --help`, extracts the suite values, and compares with the `smoke-*` ids both ways. If the help has no list, that is a defect in the acceptance issue that added `--suite`; report it, do not parse the source.
- 18 collects `-p <name>` and `--test <name>` from every command (`grep -o -- '-p [a-z0-9_-]*'`, the same for `--test`), compares with `cargo metadata --no-deps --format-version 1` package names and test target names, checks that every `tools/...` and `crates/...` path in a command exists, and that every `target/release/<name>` is a `bin` target name from the same metadata.

### 6.7 Case 23 (workflow)

Text assertions with `grep`/`awk` on `.github/workflows/release-candidate.yml`. Do not add a YAML parser. The `uses:` lines must each end with `@` and 40 hex characters (a version tag is not enough). Each job has a `timeout-minutes:` line. The check that `ci.yml` is unchanged compares `HEAD` with `git merge-base HEAD "${RC_TESTS_BASE:-origin/main}"`, not with the moving branch tip, so a later change to `ci.yml` on `main` cannot fail it. When the ref does not exist (a bare checkout) the case prints `CASE workflow-file-shape: FAILED: set RC_TESTS_BASE` rather than skipping. The workflow sets `RC_TESTS_BASE` to the pull request's base commit, so CI and a local run compare the same way.

### 6.8 What no case may do

Run `cargo test`, `cargo build` or any gate of the real `gates.tsv` other than through cases 19 to 22's extracted commands on temporary inputs (case 17 builds the `smoke` help, cases 18 and 19 call `cargo metadata`; these are the only cargo calls); call the network; need a gateway, Herdr or a profile; write inside the real repository.

## 7. The workflow file

`.github/workflows/release-candidate.yml`. Sketch (the builder copies the real action references, the toolchain step and the `cargo-typify` install from `ci.yml` and `contract.yml`, and pins by hash; the hashes below are placeholders):

```yaml
name: release-candidate
on:
  pull_request:
    paths:
      - 'tools/release/**'
      - '.github/workflows/release-candidate.yml'
  workflow_dispatch:
permissions:
  contents: read
jobs:
  rc-tools:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    env:
      RC_TESTS_BASE: ${{ github.event.pull_request.base.sha }}
    steps:
      - uses: actions/checkout@<40-hex hash used by ci.yml>
        with:
          fetch-depth: 0
      # toolchain step exactly as ci.yml has it
      - run: sh tools/release/tests/rc.test.sh
  rc-linux:
    if: github.event_name == 'workflow_dispatch'
    runs-on: ubuntu-latest
    timeout-minutes: 90
    env:
      GH_TOKEN: ${{ github.token }}
    steps:
      - uses: actions/checkout@<40-hex hash used by ci.yml>
        with:
          fetch-depth: 0
      # toolchain step as ci.yml; cargo-typify install as contract.yml
      - run: git rev-parse HEAD
      - run: sh tools/release/rc.sh run --class ci --evidence "$RUNNER_TEMP/evidence.md" --summary "$RUNNER_TEMP/summary.md"
      - if: always()
        run: cat "$RUNNER_TEMP/summary.md" >> "$GITHUB_STEP_SUMMARY"
      - if: always()
        uses: actions/upload-artifact@<40-hex hash>
        with:
          name: rc-linux-evidence
          path: |
            ${{ runner.temp }}/evidence.md
            ${{ runner.temp }}/summary.md
```

Notes:

- On a dispatch `RC_TESTS_BASE` is empty in `rc-tools`, which does not run then; inside `rc-linux` the `release-tooling-tests` gate runs the test script with `RC_TESTS_BASE` unset, so case 23 falls back to `origin/main`, which the full-depth checkout has.
- `rc-linux` needs network for the three contract gates; a network failure there is BLOCKED (environment): the operator dispatches again on the same commit.
- `rc-linux` runs `bench` on Linux, as [`P1-11`](https://github.com/infiquetra/talaria/issues/208)'s `bench` job already does on every pull request ([`P1-11`](https://github.com/infiquetra/talaria/issues/208) REQ-20, default runner `ubuntu-latest`). The macOS reading comes from the local run.
- Timeout: 90 minutes is a starting value, not a measurement. The class holds a full test run, a release build of every target, `bench`, two drift gates (each regenerating, building and testing a temporary workspace copy) and the tooling tests. After the first dispatched run, set `timeout-minutes` to twice the measured duration rounded up to ten, in the same pull request or a follow-up before the freeze, and record the measured time in the pull request.
- Job names contain no matrix key. If none of the existing workflows has an `upload-artifact` use to copy a hash from, pin the newest stable release's commit hash and say so in the pull request.
- The summary and evidence files live in `$RUNNER_TEMP`, outside the workspace, so the "refuse to write inside the repository" rule holds.

Who runs it: only the operator dispatches a workflow (the parent's decision 7; its settlement C-8). The lead posts on this issue the candidate's 7 hex and the exact command, `gh workflow run release-candidate.yml --repo infiquetra/talaria --ref main`; if `main` moved past the candidate, the lead first pushes a branch at the candidate and names it in `--ref <that branch>`. The operator runs the command. The lead then finds and checks the run read-only: `gh run list --repo infiquetra/talaria --workflow release-candidate.yml --commit <candidate> --json databaseId,conclusion,event,url`, and the run's head commit must equal the candidate (`gh run view <id> --json headSha --jq .headSha`).

## 8. Edge cases

| Situation | Expected behaviour |
|---|---|
| Gate prints no output | Evidence block holds `(no output)`; PASS or FAIL by exit code alone |
| Gate prints more than 40 lines | Evidence keeps the last 40; the full output is not kept anywhere |
| Gate exits 77 from a real failure | Counts as SKIP, so the run fails; the reason line shows what it was |
| Gate edits a tracked file or makes an unignored file | FAIL `(changed the working tree)`, the runner stops (3.6) |
| Gate is killed by Ctrl-C | The runner stops (no trap needed); no result line; nothing is written (evidence is written at the end) |
| Two `--only` ids in the wrong order | File order is used |
| `--only` of an informational gate | Allowed; `RC_PARTIAL`; its failure does not change the exit code |
| `--class live --only fmt` | Empty selection: `rc.sh: no gate selected`, exit 2 |
| `--gates` pointing at a copy of the real file | `RC_PARTIAL`, never `RC_RESULT` |
| `--class live` | Excludes `live-pause`; `--class operator` selects only it |
| Gates file with CRLF line endings | Exit 2 with the line number |
| `$HOME` contains the repository root | Replace the longer string first, so `<repo>` appears and `<home>` does not eat it |
| The temporary directory appears in both spellings | Both are replaced by `<tmp>` |
| A gate prints a path under another user's folder or a non-default `CARGO_HOME` | Step 4 of the scrub rule turns it into `<home>` |
| The Linux run and the macOS run are on different commits | The closeout is invalid; both must be the candidate |
| `origin/main` moved after the freeze | Fine for the run; the closeout still names the candidate |
| REL-02's documentation merges after the freeze | `allowed-diff` accepts `docs/`, `README.md` and `llms.txt` |
| A fix for a failed gate is merged | New candidate; section 11 restarts from step 3 |
| `cargo-typify` missing | `contract-check` prints `cargo-typify is not installed` and exits 77: SKIP, the run fails; install, then run everything again |
| `gh` not signed in or no network | The three fetching contract gates exit 1 with `contract_gen.py`'s plain line: BLOCKED (environment) |
| `python-final` tag missing locally | `no-attribution-lines` fails; `git fetch --tags origin`, then run again |
| Herdr not the calling shell, or no `tmux` for `phase2a` | The smoke gates exit 2 with their own plain sentence and FAIL: BLOCKED (environment); run again from a Herdr pane |
| The operator has not allowed `live-pause` | Do not start the full run; ask on the issue |

## 9. Closeout template and evidence rules

Post as one comment on the REL-01 issue, after the full run, the Linux run and the operator drive on the same commit:

```text
RC-CLOSEOUT
Candidate: <40 hex>
Hermes: pinned <tag>; newest stable that day <tag>
macOS: <version>, <architecture>
RC_RESULT: PASSED (<p> of <n> required gates pass, <k> informational gates not passing, commit <7 hex>)
Full run elapsed: <minutes> min
Linux CI run: <run url> (conclusion: success)
Operator drive: <comment url>
Latest-Hermes drift: <one line: PASS, or FAIL with the first failing symbol>
Known issues: <defect issue numbers with the operator's decision, or "none">
```

followed by the summary table and a collapsed section with the evidence. Rules: never an absolute path, user name, host name, ticket, key, or a profile name other than `default` or `fixture`; the evidence file is already scrubbed, but read it once before posting. `macOS: <version>` comes from `sw_vers -productVersion`, the architecture from `uname -m`. The operator drive line links the operator's own comment, whose first line names the same candidate.

## 10. Design reasoning and rejected alternatives

- Shell, not a Rust crate. The gates are commands; the runner's job is to run them, time them and print lines. A crate would add a dependency tree and a build before the first gate. Cost: a shell script needs `shellcheck` and disciplined POSIX use; the tests run on both platforms.
- Gates in a data file. A new suite adds a line, not code. Case 17 fails the build if a suite has no gate.
- Run all gates, do not stop at the first failure (except when a gate changed the tree, because every later result would be about a different tree).
- SKIP never passes a required gate. The Python era's acceptance counted skipped checks as passes; `TALARIA_LIVE_REQUIRE=1`, the SKIPPED check in `live-tests` and this rule close that hole at three levels.
- Missing preconditions keep each tool's own exit code. Rejected: rewriting smoke's and bench's exit 2 into 77 inside the gate text. Either way the run fails; rewriting would hide which tool said what and could reclassify a real failure (for `contract_gen.py`, 2 is a usage mistake). The README's exit-code map tells the lead which codes are environment trouble.
- Result words differ by selection, and `--gates` is always partial. A partial or hand-made run must not be quotable as release proof; the cheapest enforcement is a different word at the start of the line.
- A separate document scan. Rejected: pointing [`P1-04`](https://github.com/infiquetra/talaria/issues/201)'s scanner at `docs` (it reads only `*.jsonl` directly in a folder, so it read nothing there); changing that scanner (it is [`P1-04`](https://github.com/infiquetra/talaria/issues/201)'s code and contract). A small owned script with five line rules is enough for documents.
- `live-pause` as an operator gate. Rejected: running it inside `live-tests` (without `TALARIA_LIVE_PAUSE` it skips and passes, so the gate would prove nothing); setting `TALARIA_LIVE_PAUSE` for everyone (it pauses spawning for every session on the gateway, which the operator controls).
- No committed record. ADR 0012 decision 4 bars release-record tests. A comment is dated, linked from the issue, and shows an edit mark when changed. If the operator wants a file, add it as a requirement first.
- `allowed-diff` instead of "tag whatever is on `main`". The version bump and changelog must change files after the candidate. Rejected: re-running the live gates on the tag commit (hours for a version string); freezing version and changelog into the candidate (a changelog written before the proof lists unproven things). Rejected: masking any `version =` line without looking at its table (a `[dependencies.serde]` version bump would then pass).
- Rejected: a publish step or the tag in the runner. Tags and releases belong to [`REL-03`](https://github.com/infiquetra/talaria/issues/284).
- Rejected: loosening any budget, or treating a Linux budget failure as a machine difference. [`P1-11`](https://github.com/infiquetra/talaria/issues/208)'s `bench` job already gates pull requests on Linux with the same budgets.
- Linux only in CI, live gates local. A hosted runner has no Hermes gateway, no Herdr and no free-model key.

## 11. Runbook for the lead

Run live activities one at a time. Only the Linux run may overlap with them.

1. Preconditions (issue REQ-1): confirm each phase acceptance issue is closed as completed: `gh issue view <number> --repo infiquetra/talaria --json state,stateReason --jq '.state + " " + .stateReason'` prints `CLOSED COMPLETED`. Confirm with the operator the evidence shape, who does the drive, and how `live-pause` will be allowed.
2. Tooling: build this unit by its pull request (test author first, then developer); post the `rc-linux` dispatch command for the branch so the operator runs it once to measure the timeout (section 7); merge. Right after the merge, on `main`: `sh tools/release/rc.sh run --only no-attribution-lines`. A FAIL is reported to the operator now, not at the freeze.
3. Freeze: `git fetch origin`; `CANDIDATE=$(git rev-parse origin/main)`; `git worktree add "$X/rc" "$CANDIDATE"` where `$X` is a new temporary directory outside the repository; `cd "$X/rc"`. Post the candidate on the issue.
4. Fast feedback: `sh tools/release/rc.sh run --class ci --evidence "$X/ci-ev.md" --summary "$X/ci-sum.md"`. Fix nothing in this worktree. If a required gate fails for a product reason, go to step 10.
5. Live preconditions: ask the operator to confirm REQ-2 of the issue. Do not start the gateway yourself.
6. The run that counts, from a Herdr pane in the worktree: the operator runs `sh tools/release/rc.sh run --evidence "$X/ev.md" --summary "$X/sum.md"`, or the agent tester runs it after the operator's `RC pause: ok <7 hex>` comment. Save the pane text (`herdr pane read`) and the elapsed time.
7. The Linux run (section 7): post the dispatch command and the candidate's 7 hex on the issue; the operator runs it; find and check the run read-only. It may run while steps 8 and 9 happen.
8. The agent tester runs UAT-2 to UAT-5, one at a time.
9. The operator does the drive (UAT-6) on a build of the same commit.
10. Any required gate not PASS for a product reason: open a defect issue per gate (gate id, commit, scrubbed evidence section), assign it to the owning unit's code, merge the fix, and return to step 3 on the new `origin/main`. Environment trouble: remove the cause and repeat from step 4 on the same commit. Never re-run a product failure on the old commit to see if it passes.
11. Post the closeout (section 9). Mark [`REL-03`](https://github.com/infiquetra/talaria/issues/284) ready with the candidate commit.
12. Remove the worktree and temporary directory: `git worktree remove "$X/rc"`.

Time and cost figures are not stated here because none has been measured for this runner. Record the elapsed time of the full run in the closeout so the next release has a real number.

## 12. Notes on the six acceptance scenarios

- UAT-1 is the proof. The runner reads the whole pane, not only the last line, and checks that the number of `GATE` lines equals the number of lines in `gates.tsv` that are not blank or comments (`grep -vcE '^(#|$)' tools/release/gates.tsv`). Informational lines may say FAIL or SKIP.
- UAT-2 builds from a fresh clone on purpose: it catches files that exist only in the lead's worktree. Use a new empty folder; do not reuse the lead's `target` directory. Its 5-second bound is what a person sees on a fresh clone; the measured budget of 200 ms is `live-tests`' job.
- UAT-3's negative half uses an unknown profile name. In print mode every start-up failure, an unknown profile included, exits 4 with [`P1-12`](https://github.com/infiquetra/talaria/issues/209)'s two lines on standard error, `talaria: <what>` and `Next step: <next>` ([`P2-14`](https://github.com/infiquetra/talaria/issues/235) REQ-4 and its exit table; [`P1-12`](https://github.com/infiquetra/talaria/issues/209)'s negative scenario prints `talaria: the gateway never discovered a profile named nosuchprofile`). Standard output stays empty for the default text format. `docs/formats/print-result-v1.md` ([`P2-14`](https://github.com/infiquetra/talaria/issues/235)) documents the JSON keys of the first half.
- UAT-4 uses scratch clones only, with local commits that are never pushed. The failing launch row appears only in the evidence file, because gate output never reaches the pane (3.2). Delete both clones after.
- UAT-5 is the only scenario that depends on GitHub's state on the day.
- UAT-6 is the operator's. The agent tester prepares the build and the two background sessions only if the operator asks, and never writes the verdict. The scenario passes only on a comment naming the candidate with six yes answers and `RC drive: PASS`; the agent checks the specific comment the closeout links (its author and its first line), not any comment that happens to contain the words.

## 13. Extra pre-mortem notes

- The first full run will find at least one gate command that no longer matches the tool it calls. That is the purpose of 4.3. Budget for one correction round to `gates.tsv` before the candidate is frozen; those corrections are part of this unit's pull request, not of the candidate.
- `live-tests` may take far longer than any single suite and may contain tests that conflict when run in one pass (shared gateway state). The mitigation is splitting by test name into several gates, not skipping.
- A live gate that creates sessions leaves them in the `fixture` profile. That is acceptable: the profile is throwaway.
- `doc-scan` may hit an existing document on the first run (a quoted path in an older handoff file). The fix is a documentation pull request by the document's owner, which `allowed-diff` accepts; never an allow-list.
- The operator's time is the one risk this unit cannot remove. Schedule the freeze with them, since any fix restarts the clock.

## 14. Facts to settle at the freeze

These were open when this document was written. None blocks building the tooling; each is read from the code at the freeze.

| Fact | Where to read it | Used by |
|---|---|---|
| The suite list and its help format | `cargo run -q --locked -p smoke -- run --help` | smoke gates, case 17 |
| `talaria-bench run`'s mandatory options and the budget key names | `tools/bench/README.md`, [`P1-11`](https://github.com/infiquetra/talaria/issues/208) spec 3.2 and 7.4 | `bench`, UAT-4 |
| The Rust toolchain step | `ci.yml` ([`P0-02`](https://github.com/infiquetra/talaria/issues/195) REQ-9 picks the version) | `rc-tools`, `rc-linux` |
| The action hashes | `ci.yml`, `contract.yml`, `bench.yml` | the workflow |
| Whether the ignored live tests fit one pass, and every SKIPPED marker | a first run; `grep -n SKIPPED crates/*/tests/live_*.rs tools/*/tests/live_*.rs` | `live-tests` |
| Whether [`P1-12`](https://github.com/infiquetra/talaria/issues/209)'s budget test makes its own 160 by 48 pseudo-terminal or needs the pane to be that size | [`P1-12`](https://github.com/infiquetra/talaria/issues/209)'s `live_session.rs` | `live-tests`, REQ-2 |
| Which packages show `publish: null` | `cargo metadata --no-deps --format-version 1` | `no-registry-publish` |
| Whether existing documents trip `doc-scan` | `sh tools/release/doc-scan.sh` | `doc-scan` |
| Whether [`REL-03`](https://github.com/infiquetra/talaria/issues/284) still consumes `allowed-diff` and `--class ci` as written here (checked at drafting: its REQ-6 (a) and (b), UAT-2, UAT-3, AC-5 and AC-6 match, including the two-space `not allowed:` lines of 3.5) | [`REL-03`](https://github.com/infiquetra/talaria/issues/284)'s body | REQ-7 |
