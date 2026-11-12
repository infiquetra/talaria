# ONE-6: Run the live tests and the smoke harness on a schedule

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read the issue for what to build, which test to write, and when to stop. Use this document for the runner, the workflows, and the case counter as they are on main today. It does not restate a requirement or a test. It points at them by id.

## The runner today

`tools/release/rc.sh` usage is line 7: `rc.sh run [--class ci|live|operator|all] [--only id]... [--gates file] [--evidence file] [--summary file]`. Unknown class or an empty selection is `rc_error`, which prints `rc.sh: <reason>` and exits 2 (line 5).

`--only` sets `rc_partial=1` at line 215. The class filter is line 257. The `--only` filter is the next line. Both have to match. A class `live` run with no `--only` selects every `live` row. With `--only`, line 265 sets the result word to `RC_PARTIAL`. The class word `RC_RESULT_LIVE` is the branch at lines 270-271, and it runs only when `rc_partial` is 0. REQ-4 keeps `--only` for that reason.

The summary writer is lines 314-318. The header it prints is `| gate | required | class | result | seconds |`, then the separator line, then one row per selected gate. `case_run_summary` (`tools/release/tests/rc.test.sh:416`) already requires that header at line 432 and requires the file to be replaced, not appended, at lines 449-453. REQ-5 uses that table. The outside-repository check is lines 224-232: the parent directory must exist, and a path inside the repository is `refusing to write summary inside the repository`. That check runs before any gate (the dirty-tree check is line 262). UAT-2 is that refusal.

A gate exit of 0 is PASS, 77 is SKIP, and any other code is FAIL (lines 296-306). The run's own exit is 0 only when every selected required gate passed. REQ-4 leaves a non-zero `rc.sh` exit as a failed job.

The catalogue is `tools/release/gates.tsv`. Seven smoke rows are lines 19-25, all class `live`, each ending in `TALARIA_LIVE_REQUIRE=1 cargo run -q --locked -p smoke -- run --suite <phase>`. `live-tests` is line 26, class `live`, with `TALARIA_LIVE_REQUIRE=1` and a trailing check that a line containing `_RESULT: SKIPPED` fails the gate. `live-pause` is line 27, class `operator`. `case_gates_shape` (`rc.test.sh:677`) requires those ids, among the other catalogue ids, to exist. It does not by itself reject an extra id. REQ-5 still adds no row. AC-6 is the check that the file is unchanged.

On main, no smoke row sets `TALARIA_SMOKE_TERMINAL`. `SEAT-4C` inserts `TALARIA_SMOKE_TERMINAL=hc` on line 19 only, immediately before `TALARIA_LIVE_REQUIRE=1`. The stop condition in the issue is that re-check. The other six smoke commands stay without that variable, and a suite other than `phase1` on `hc` exits 2. That is why REQ-4 names two gates and leaves the other six to `PROOF-5`.

## What the live rule will be

`PROOF-1` is a start gate. Until it merges, `live_skip_reason` in `crates/hermes-contract/src/pinned.rs` still demands the pinned commit. After it merges, the default is the floor `OLDEST_SUPPORTED` `(0, 23, 0)`, and `TALARIA_LIVE_EXACT=1` is what selects the old commit check. `TALARIA_LIVE_PROFILE` defaults to `fixture`. REQ-4 and REQ-6 name those facts. This issue does not edit `pinned.rs`.

`crates/hermes-contract/PINNED` on main is `tag=v0.22.4` and commit `4d04303818b61bf3aa09278af48356315515965b`. This issue does not wait on `PIN-3`, so REQ-6 treats a different tag as a stop.

`tools/smoke/src/main.rs:347` reads `TALARIA_LIVE_REQUIRE`. In `tools/smoke/src/steps.rs:275-282` a skip while that flag is set becomes a failed step whose reason ends in `TALARIA_LIVE_REQUIRE=1`. The gate row already sets the flag. The smoke guide (`tools/smoke/README.md:103-104`) says a skip stops the run with exit 3, or fails with exit 1 when the flag is `1`. The phase1 deadline in that guide (line 16) is 300 seconds. That deadline is the suite's own limit. It is not the job's `timeout-minutes`. REQ-1 sets that key.

The release guide (`tools/release/README.md:18`) still says live checks need the pinned gateway and a Herdr pane. `PROOF-1` and `SEAT-4C` are what change that for the two gates this job runs. This issue does not edit the guide.

## Workflows on main

Seven workflow files exist under `.github/workflows`: `bench.yml`, `ci.yml`, `contract.yml`, `package-build.yml`, `package-check.yml`, `release-candidate.yml`, `release.yml`. None is named `live-schedule.yml`. None contains `self-hosted`. The `runs-on` values are `ubuntu-latest`, `macos-15`, and the `macos-14` matrix entry in `ci.yml:32` which line 33 rewrites to `macos-15`. None of those runners has a gateway or `hc`. REQ-1 does not use those labels.

The only `schedule` on main is `contract.yml:16-17`, cron `17 6 * * 1`, plus `workflow_dispatch` at line 18. Permissions at lines 20-21 are `contents: read`. The drift job reports with `--summary "$GITHUB_STEP_SUMMARY"` at line 105. That program is `contract_gen.py`, not `rc.sh`.

The one caller of `rc.sh` from a workflow is `release-candidate.yml:36-38`. It passes `--summary "$RUNNER_TEMP/summary.md"`, then, with `if: always()`, appends that file to `$GITHUB_STEP_SUMMARY`. The job `rc-linux` has `timeout-minutes: 90` (line 26) and runs `--class ci` on `ubuntu-latest`. REQ-3 and REQ-4 follow the summary path. REQ-3 names five steps. The third is the command of the `release-build` row in `tools/release/gates.tsv`. REQ-1 does not copy the 90 or the hosted label. Checkout in both of those workflows uses `actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09`. `case_workflow` (`rc.test.sh:1096-1102`) requires every `uses:` line in `release-candidate.yml` to carry 40 hexadecimal digits. REQ-3 uses the same pin. The new case applies the same pin check to the new file and requires that third step.

`SYNC-4` will add `.github/workflows/hermes-sync.yml` with cron `23 7 * * *` and will open a GitHub issue on failure. That file is not on main. REQ-1 forbids that cron string. REQ-2 grants no `issues` permission. This issue does not take `SYNC-4` as a dependency.

`case_workflow` also fails if `.github/workflows/ci.yml` differs from the merge base (`rc.test.sh:1111-1113`). AC-6 leaves that file unchanged so the existing case stays green.

## The case counter

`rc.test.sh` calls 23 cases, from `case_run_all_pass` at line 1128 through `case_workflow` at line 1150. Line 1152 requires `NRAN` to equal 23. Line 1157 prints `RC_TESTS: PASSED (23 cases)`. The test named in the issue inserts `case_live_schedule` after line 1150 and changes those two 23s to 24 in the same commit. A new case that leaves the 23s in place fails the counter even when the new case would pass, which hides the missing-file failure.

`begin` increments `NRAN` (lines 24-26). `fail` increments `NFAIL` and does not exit by itself (lines 19-22). The missing-file branch must `return` after `fail`, as `case_workflow` does at lines 1064-1067, so the script reaches the counter and then exits 1 because `NFAIL` is not 0.
