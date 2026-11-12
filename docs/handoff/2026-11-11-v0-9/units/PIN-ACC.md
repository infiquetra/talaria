# PIN-ACC: Live proof: the suite and the live tests pass at the new Hermes pin

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for the three gates and the record they fill. This page maps those gates. It does not restate the requirements.

## What exists today

`tools/release/gates.tsv` is four tab-separated fields: id, required, class, command. The `tests` row runs `cargo test --workspace --locked` (`gates.tsv:4`). The `contract-pinned-drift` row runs `drift --tag` of the `tag=` line in `crates/hermes-contract/PINNED` (`gates.tsv:11`). The `live-tests` row runs the ignored live tests with `TALARIA_LIVE_REQUIRE=1` and fails if the output contains `_RESULT: SKIPPED` (`gates.tsv:26`).

`sh tools/release/rc.sh run --only <id>` selects one gate. It refuses a dirty tree (`rc.sh` checks `git status --porcelain` before a run). `--only` prints `RC_PARTIAL`. Exit 0 prints `RC_PARTIAL: PASSED`. A passing gate prints `GATE <id> PASS`. Stdout is those lines, not the captured command output. `--evidence` writes the last 40 lines of that output, and it refuses a path inside the repository.

`PROOF-4` creates `docs/acceptance/v0.9.0/pin.md` with the title `# pin acceptance record`, empty `Candidate:`, `Hermes:`, `Herdr:`, `OS:`, and `Date:` lines, and a table with no data rows. The columns are `id`, `persona`, `result`, `evidence`, `commit`.

`PROOF-1` makes the live tests print `gateway code_version=<version> code_sha=<sha>` and accept a gateway at or above `OLDEST_SUPPORTED`, `(0, 23, 0)`. `PIN-5` is already on `main`, so the workspace suite includes its tests.

## Why the record is written after the gates

The runner stops when the work tree is dirty. The three gates run on the candidate commit. `pin.md` is filled from their output and committed afterwards. `Candidate` names the commit the gates ran on, not the commit that adds the record. The commands someone types to start Talaria, `doctor` and `--version`, use the release binary copied to `/tmp/talaria-<commit>/talaria`, and that process starts in `/tmp/talaria-<commit>/run`. The `live-tests` row is a `cargo test` command. It needs no typed path and no run folder. The binary it runs is the one cargo builds from the candidate checkout, and the evidence of that build is `git rev-parse HEAD`, recorded as `Candidate:`. After `PROOF-1`, `live_test_skip` uses the version floor unless `TALARIA_LIVE_EXACT` is `1`. Today's `live_skip_reason` (`pinned.rs:7-23`) still requires the pinned commit; that call is replaced before this issue starts. `live_attach_cmd.rs` skips when the gateway does not serve `fixture`. `rc.sh` runs each gate with `sh -c` (`rc.sh:284`). The operator's comment includes `grep -m1 '^gateway code_version='` of `live.txt`. That line is `gateway code_version=<version> code_sha=<sha>`. `crates/talaria-cli/tests/support/mod.rs` starts `env!("CARGO_BIN_EXE_talaria")` in a per-test temporary project. `live_session.rs` and `live_attach_cmd.rs` pass `--profile fixture`. The `live-tests` row of `gates.tsv` does not mention `TALARIA_LIVE_PROFILE`. `rc.sh` prints `GATE` and `RC_PARTIAL` lines. Its `--evidence` output is not the source of `gateway code_version=`. The record header stays the five lines `PROOF-4` created, and a data row uses the same pipe spacing as `| id | persona | result | evidence | commit |`.
