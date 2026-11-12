# PROOF-4: Release gate rows and the v0.9.0 acceptance record

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read the issue for what to add. Use this document for the catalogue that exists today. It does not restate a requirement or a test.

## The catalogue today

`tools/release/gates.tsv` is tab-separated. The header is `id`, `required`, `class`, `command` (line 1). There are 26 data rows. `contract-latest-drift` is the only row whose required field is `no` (line 12). `replay-fixtures` (line 6) replays every `fixtures/frames/*.jsonl` except `p5-05-progress.jsonl`, with `cargo run -q --locked`. `live-tests` (line 26) is class `live`, required `yes`, and sets `TALARIA_LIVE_REQUIRE=1`. Its command skips `live_pause_blocks_new_spawns` and rejects `_RESULT: SKIPPED`. `live-pause` (line 27) is class `operator` and sets `TALARIA_LIVE_PAUSE=1`. Seven `smoke-phase` rows are lines 19–25. REQ-1 does not change those commands.

`tools/release/rc.sh --list` prints the file and exits 0 (`rc.sh:23-27`). `rc.sh run --only` of an unknown id exits 2 with `rc.sh: unknown gate` (`rc.test.sh` `case_run_usage`).

`tools/release/tests/rc.test.sh` ends with a comparison of `NRAN` to 23 (line 1152) and a printed `(23 cases)` (line 1157). `begin` increments `NRAN` (line 26). `case_gates_shape` requires the 26 known ids to exist and does not reject an extra id (lines 683–708). It fails a `cargo` command that omits `--locked` (lines 743–760). It fails if the `live-tests` command drops `live_pause_blocks_new_spawns` (lines 763–766). `$GATES`, `$RC`, and `$ROOT` point at the real checkout (lines 5–9). `run_rc` runs inside a temporary repository that has no copy of the real gates file. REQ-4 does not call `run_rc`.

## The sentence to reword

`tools/release/README.md:18` says live checks need `the pinned Hermes gateway`, then a configured fixture profile, a Herdr pane, tmux, and jq. That phrase is not in `tools/release/gates.tsv`. REQ-2 replaces only those words. Line 20 says exit 0 is PASS, exit 77 is SKIP, and exit 2 is invalid input. `TALARIA_LIVE_EXACT` is not read anywhere in this checkout. `PROOF-1` adds the reader. REQ-1 only sets the variable on the new row.

## The v0.7.0 header

`docs/acceptance/v0.7.0/phase-1.md:1-10` is a title, then `Candidate:`, `Hermes:`, `Herdr:`, `OS:`, and `Date:`, then the table header. Line 3 is `Hermes: v0.22.4`. Line 11 is the first data row, and its result is `PASS`. REQ-3 keeps the labels and the columns, leaves the header values empty, and adds no data row. `docs/acceptance/v0.9.0/` does not exist yet.
