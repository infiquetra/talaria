# SEAT-4B: Smoke harness: choose the terminal, and run the first suite on hc

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: start from the issue. This file is the map of the smoke harness as it is today, with line numbers. It does not restate a requirement or a test. Those are REQ-1 through REQ-7 and the tests in `tools/smoke/tests/selector.rs`.

## What the binary does today

`tools/smoke` is the package `smoke` (`tools/smoke/Cargo.toml:2`). The binary is `smoke run` (`tools/smoke/src/main.rs:74-77`). A preflight error is printed on standard error and the process exits 2 (`main.rs:85-88`).

`Cli` defaults `--herdr` to `herdr` (`main.rs:45-46`), `--profile` to `fixture` (`main.rs:48-49`), `--model` to `minimax-m3.1-flash` (`main.rs:51-52`), `--provider` to `ollama-cloud` (`main.rs:54-55`), and `--suite` to `phase1` (`main.rs:66-68`). The suite parser rejects an unknown name with `unknown suite '{s}'; use phase1 or phase2a` (`tools/smoke/src/suite.rs:28-40`). REQ-2 uses that same shape for the selector.

The pane sentence is the constant `NOT_IN_HERDR` (`main.rs:29-31`): `This test must run inside a Herdr pane (HERDR_ENV=1). Start it from a Herdr tab.` It is returned when `HERDR_ENV` is not `1` or `HERDR_WORKSPACE_ID` is empty (`main.rs:126-132`). `tools/smoke/tests/cli.rs::outside_herdr_exits_2_no_tab` (`cli.rs:134-146`) asserts that sentence. REQ-3 leaves the test and the sentence alone.

After the pane check, `main.rs:215-219` builds `HerdrCli::new` with the `--herdr` path and the workspace, then calls `HerdrCli::check`. `check` is an inherent method (`tools/smoke/src/terminal.rs:119`), not a method of the `Terminal` trait (`terminal.rs:51-75`). The trait's methods stop at `version`. There is no `wait` method. `pane_get` has a default that returns `pane get is not supported` (`terminal.rs:68-70`). `HcCli` is not in this tree.

`main.rs:362` prints the prefix `herdr:` plus `version()`. A failed run writes the same prefix at `main.rs:439`. REQ-4 names the line each path prints. `main.rs:365-366` passes `&herdr` to `run_smoke` for `phase1`. Every other suite in that match receives the same `HerdrCli` (`main.rs:367-407`).

## Why the config struct cannot grow a field

`SmokeConfig` (`tools/smoke/src/config.rs:7-21`) has no selector field and no executable field. Thirteen literals construct it: one in `main.rs:328` and twelve under `tools/smoke/tests/`. The implementation commit cannot edit a test file. A new field changes every one of those literals.

## Where phase1 polls

`POLL_INTERVAL` is 250 milliseconds (`tools/smoke/src/screen.rs:18-19`). The phase1 loops that sleep that interval are in `tools/smoke/src/steps.rs`:

- `interrupt`, the counting loop (`steps.rs:566-585`) and the settle loop (`steps.rs:617-650`). The one-second sleep at `steps.rs:589` is not `POLL_INTERVAL`.
- `quit` (`steps.rs:673-739`)
- `resume`, the second pane's quit loop (`steps.rs:828-855`)
- `wait_for_shell` (`steps.rs:873-906`), which also calls `foreground`
- `wait_for_screen` (`steps.rs:910-950`), which also calls `foreground`
- `wait_for_reply` (`steps.rs:954-999`). The text it counts is `Token::join` (`screen.rs:54-56`), the form `word-number`.
- `watch_approval` (`steps.rs:1004-1093`)
- `wait_for_deletion` (`steps.rs:1098-1122`)

`run_smoke` takes `&dyn Terminal` (`steps.rs:186-190`). It calls `create_tab` before any step (`steps.rs:227-236`). `resume` calls `split_pane` (`steps.rs:749-752`). `quit` calls `foreground` (`steps.rs:687-692`). `wait_for_shell` and `wait_for_screen` also call `foreground`. The issue does not change these loops and does not call `hc wait`.

## Results and gates

`SmokeReport::result_line` returns `SMOKE_RESULT: PASSED (elapsed N s)` only when nothing failed and nothing skipped (`tools/smoke/src/report.rs:90-96`). A skip returns `SMOKE_RESULT: PASSED WITH SKIPS (...)` (`report.rs:91`). The exit code is 0 for a clean pass and 3 when a step skipped (`report.rs:60-65`). The README says the approval step skips, and the run exits 3, when the profile does not ask for approval itself (`tools/smoke/README.md:34-36`). Exit codes are 0 passed, 1 failed, 2 preflight failed, 3 passed with a skip (`README.md:53`).

`tools/release/gates.tsv:19-25` are `smoke-phase1` through `smoke-phase5`. Each command runs one suite with `TALARIA_LIVE_REQUIRE=1`. None of those lines sets `TALARIA_SMOKE_TERMINAL`. REQ-3 does not edit the file. [#538](https://github.com/infiquetra/talaria/issues/538) (`SEAT-4C`) later sets `TALARIA_SMOKE_TERMINAL=hc` on the `smoke-phase1` row only. The other six smoke rows stay as they are.

The recorded multiplexer version in the tree is the single line `herdr 0.9.4` at `tools/smoke/tests/fixtures/herdr/version.txt:1`. This issue does not change that file.

## Documents

- [Smoke README](../../../../tools/smoke/README.md)
- [Argument parsing and the pane check](../../../../tools/smoke/src/main.rs)
- [Phase1 steps](../../../../tools/smoke/src/steps.rs)
- [Terminal trait](../../../../tools/smoke/src/terminal.rs)
- [Smoke config](../../../../tools/smoke/src/config.rs)
- [Suite names](../../../../tools/smoke/src/suite.rs)
- [Result line](../../../../tools/smoke/src/report.rs)
- [Release gates](../../../../tools/release/gates.tsv)
- [Pane-sentence test](../../../../tools/smoke/tests/cli.rs)
