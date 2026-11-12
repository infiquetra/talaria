# SYNC-3: One command moves the Hermes pin

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the generator that exists today, for the two Hermes files the command is allowed to read, and for why a failed run must not touch the source workspace. The requirements, the tests and the acceptance checks are only in the issue. This page points at them by id.

## What the generator does today

`tools/contract-gen/contract_gen.py` is Python 3.9, standard library only. `ROOT` is two parents above the script (`contract_gen.py:17`). The crate is `ROOT / "crates/hermes-contract"` (`contract_gen.py:18`). Generated Rust goes to `crates/hermes-contract/src/generated` (`contract_gen.py:20`).

The public repository string is `NousResearch/hermes-agent` (`contract_gen.py:22`). The contract path inside that repository is `apps/shared/src/gateway-contract.openrpc.json` (`contract_gen.py:23`). Both that path and `LICENSE` exist at Hermes v0.23.0. REQ-2 reads those two and no other file.

`main` (`contract_gen.py:710`) accepts `generate`, `check`, `verify-vendor`, `pin`, and `drift`. The pin parser's help text is "validate and print the current pin; does not bump it" (`contract_gen.py:722`). There is no `bump` or `tested` sub-command in this tree. `compat` is provided by `SYNC-1` (start) and is on `main` before this issue starts. This issue calls `main(["compat", "--write"])` and does not implement it. It does not call that sub-command through `run()`, because `run()` keeps stdout only on success. Tests 1 to 4, 7 and 9 fail before the change because `bump` is not a sub-command. Tests 5, 6 and 8 fail because `tested` is not a sub-command. The process exits non-zero.

`gh_raw` (`contract_gen.py:570-571`) runs `gh api` with `Accept: application/vnd.github.raw+json` and `repos/NousResearch/hermes-agent/contents/<path>?ref=<commit>`. `verify_vendor` (`contract_gen.py:574-577`) already fetches the pair this command may fetch:

```python
for local, remote in ((Path(contract_path), CONTRACT_PATH), (CRATE / "vendor/LICENSE.hermes", "LICENSE")):
```

`resolve_tag` (`contract_gen.py:587`) turns a tag into `(tag, commit)` through the GitHub API. That call is not a third file. `version` (`contract_gen.py:95-99`) accepts only `v<major>.<minor>.<patch>`. `compare_versions` (`contract_gen.py:121-122`) is positive when the left tag is newer. The existing pin test asserts `compare_versions("v0.22.10", "v0.22.4") > 0` (`tools/contract-gen/tests/test_pin.py:29`). REQ-3 and REQ-6 use that direction.

`validate` raises `ContractError("inline string enum title collision: {}".format(title))` when two inline string enums share a title and differ in values (`contract_gen.py:204-205`). REQ-5 prints the case phrase `a title collision in the generator` when that text is present. `SCHEMA_REMOVALS` (`contract_gen.py:31-41`) is the list a person extends when a title collision is real. REQ-5 forbids this command from editing it.

`generate` (`contract_gen.py:506`) validates, then runs `cargo typify` and `rustfmt`, and writes six files: `mod.rs`, `schemas.rs`, `methods.rs`, `events.rs`, `server_requests.rs`, `native.rs`. A Hermes checkout is optional and, when set, parses Hermes source (`contract_gen.py:511`). REQ-2 calls `generate` with no checkout, so nothing from Hermes is executed.

`drift` (`contract_gen.py:652`) resolves a tag, fetches the contract, and generates inside a temporary directory. The README says the source workspace is never modified (`tools/contract-gen/README.md:27-29`). `copy_workspace_for_drift` (`contract_gen.py:637-646`) copies only `hermes-contract` and rewrites the manifest members to that one crate. REQ-2 does not use that helper for the check, because `cargo check --workspace --all-targets` has to see the other crates.

Tests 1, 2, 3, 5, 6, 7, 8 and 9 keep `gh` off the network with `support.install_fakes` (`support.py:269`) and set the child `PATH` with `support.prepend_path` (`support.py:277`). Test 4 does not. It writes `cargo` and `cargo-typify` with `support.write_executable` (`support.py:188`) of `support.fake_cargo_script` (`support.py:238`) and passes that directory alone as `PATH` in `support.run_script`'s env (`support.py:167-170`). It does not set `os.environ`. The fake `gh` serves `GH_CONTRACT` and `GH_LICENSE` and logs every argv line (`support.py:194-234`). `fake_cargo_script` logs every argv line, including `--version` and `typify`. `support.run_script` launches the script with `python3 -I` (`support.py:171`). REQ-1 and the nine tests follow that.

## The pin files a successful bump rewrites

`crates/hermes-contract/PINNED` is two lines (`PINNED:1-2`):

```text
tag=v0.22.4
commit=4d04303818b61bf3aa09278af48356315515965b
```

`parse_pinned` (`contract_gen.py:107-114`) requires that shape and a tag of `v0.22.3` or later. `PIN-3` replaces these two lines before this unit starts. REQ-3 writes the same shape for the new tag.

`crates/hermes-contract/src/pinned.rs:3-4` holds `pub const TAG: &str = "v0.22.4"` and `pub const COMMIT` as a string literal. There is no `OLDEST_SUPPORTED` in this tree. `PROOF-1` adds it. REQ-3 changes those two lines once, to `crate::compat::CURRENT.pinned_tag` and `crate::compat::CURRENT.pinned_commit`, and no other line, so a constant added above or below them survives. `bump` does not write the file. No Rust or Python file reads the source text of `pinned.rs`. Callers use `hermes_contract::pinned::TAG` and `hermes_contract::pinned::COMMIT`, or they read `PINNED`. `crates/hermes-contract/tests/pinned.rs` compares the constants with `PINNED`. This issue does not edit those callers.

The vendored bytes are `crates/hermes-contract/vendor/gateway-contract.openrpc.json`, `gateway-contract.openrpc.json.sha256`, and `LICENSE.hermes`. The checksum test (`tools/contract-gen/tests/test_pin.py:41-44`) requires one line, 64 lowercase hex, two spaces, then the contract file name. REQ-3 keeps that line.

The newest table entry is the last `[[release]]` table, with keys `talaria`, `floor`, `pinned_tag`, `pinned_commit`, and `newest_tested`. `PIN-2` creates `crates/hermes-contract/compat.toml`. `PIN-3` (start) adds the 0.9.0 entry (floor `v0.23.0`, pinned `v0.23.1`, newest tested `v0.23.1`) and that entry is on `main` before this issue starts, so `compat.toml` is on `main` before this issue starts. `SYNC-1` (start) does not create the file and is on `main` before this issue starts. `generate` writes `crates/hermes-contract/src/generated/compat.rs` with the other generated files. That file defines `pub struct CompatRow { pub talaria: &'static str, pub floor: &'static str, pub pinned_tag: &'static str, pub pinned_commit: &'static str, pub newest_tested: &'static str }` and `pub const CURRENT: CompatRow` (the newest entry). `compat --write` and `compat --check` regenerate only `docs/compatibility.md` and the README section "Hermes compatibility". Consumers read `hermes_contract::compat::CURRENT`. Nothing parses `compat.rs` at run time. This issue calls `compat --write` and does not hand-edit `compat.rs`. The markers are `<!-- hermes-compat:start -->` and `<!-- hermes-compat:end -->` in `README.md`, immediately before `## Install` (`README.md:8`). The workspace package version is `0.8.0` (`Cargo.toml:20`). The tests write `talaria = "0.8.0"` themselves and assert the command leaves that cell alone. After `_prepare_workspace` copies the repository file, they write the `v0.22.4` pin pair above, so the fixture does not follow the pin `PIN-3` lands.

## Hand-written Rust this command must not repair

`crates/hermes-contract/src/events.rs:189-190` maps `"browser.controller.cancel"` to `BrowserControllerCancel` and `"browser.controller.command"` to `BrowserControllerCommand`. At the current pin those names are still in the tree. REQ-5's second case is what a later contract looks like when `cargo check` names this file.

Two struct literals name their fields and have no `..` fill. `client_capabilities::Params` is built with `server_requests: true` only (`crates/hermes-client/src/connect.rs:534-536`). `RollbackListParams` is built with `profile` and `session_id` (`crates/talaria-app/src/rewind.rs:92-95`). REQ-5's fourth case is a check failure on a literal of that shape (`error[E0063]` is the compiler's missing-field code). The command prints the case. A person edits the literal.

A failure whose path contains `/tests/` is the third case: a recorded frame or a mock compiled under a tests directory. `cargo check --workspace --all-targets` compiles those targets. It does not run them. A mock that fails only at run time is outside this command. `SYNC-4` is the unit that runs `cargo test --workspace`.

## Why a failed run restores the workspace

A failed validation, generation, or check leaves the working tree as it is. `drift` gets that result by working in a temporary directory (`contract_gen.py:670`). Its helper `copy_workspace_for_drift` keeps only `hermes-contract` (`contract_gen.py:641-646`), so `cargo check --workspace --all-targets` run there would not see `hermes-client` or `talaria-app`. REQ-2 therefore checks this workspace, after remembering each output path's bytes, and restores those bytes if a later step fails. A path that was absent is removed. A missing `gh` never writes. `run` raises `ContractError("{} is not installed".format(args[0]))` when the binary is absent (`contract_gen.py:86-87`), which for `gh` is the line `gh is not installed`. `resolve_tag` (`contract_gen.py:602-603`) rewrites every `ContractError` as a missing tag. REQ-4 says not to rewrite `gh is not installed`. That except serves every caller, so a missing `gh` makes `drift` print `gh is not installed` instead of the missing-tag line. No current test pins the old `drift` text. The missing-tag test still expects `tag v0.0.0-missing was not found in NousResearch/hermes-agent`. REQ-5 matches the full stderr of `cargo typify`, `rustfmt`, and `cargo check`. `run` keeps that stderr for the matcher and still stores only the last three lines in `ContractError` (`contract_gen.py:89-91`). The printed error stays one line. A `validate` failure matches the `ContractError` text. The licence has no validator. The last `[[release]]` block is edited as text. A file that does not parse, or a `recorded_against` value `version()` rejects, is skipped. A directory that was already there is left in place. Every `bump` test except test 4 sets `GH_COMMIT`.

The tests build a temporary workspace and run the copied script, so `ROOT` for that process is the temporary workspace (`support.py:181-185`). They do not need the other crates: fake `cargo` exits 0 for `check`.

`tested` never calls `gh`. An older or equal tag is not newer, so `compare_versions` is not positive and REQ-6 leaves the file bytes alone. If `compat --write` fails after the cell is set, REQ-6 puts `compat.toml` and both copies back to their prior bytes and exits 1. Raising newest-tested on `bump` uses the same comparison against the table cell, not against the old pin.

On success, `bump` lists paths with `git ls-files` and prints each whose `recorded_against` release is older than the new pin. If `git` fails, it prints no paths and does not change the exit status. `PIN-2` names the JSON key `recorded_against` (a string tag) in `fixtures/configuration-surface/matrix.json`, `fixtures/configuration-surface/static-rows.json`, and `fixtures/gateway-concurrency/results.json`. REQ-8 scans tracked `.json` files under `crates/` and `fixtures/` whose top-level value is an object with that string key. A nested key does not count, and no other syntax exists. A top-level value that is not a string is skipped, and `version()` is not called on it. The exit status does not depend on the list, and the command does not change those files. `SYNC-4` repeats the list in the issue the watcher opens. That workflow is not on `main` before this issue starts.

## Edge cases the tests pin

| Situation | Result | Where it is decided |
| --- | --- | --- |
| New pin newer than `newest_tested` | `newest_tested` becomes the pin tag | REQ-3, test 1 |
| New pin older than `newest_tested` | `newest_tested` stays | REQ-3, test 2 |
| New pin equal to `newest_tested` | `newest_tested` stays | REQ-3, `compare_versions` is 0 |
| `tested` tag newer | that cell changes, pin files do not, `gh` log empty | REQ-6, test 5 |
| `tested` tag older or equal | exit 1, `newest tested is already <current>`, bytes unchanged | REQ-6, test 6 |
| `tested` tag `version()` rejects | exit 1, `invalid stable tag <tag>`, nothing written | REQ-6 |
| No `[[release]]` entry | exit 1, `SYNC-1`'s missing-entry `ContractError`, no `gh` call, nothing written | REQ-4 |
| Top-level `recorded_against` is not a string | skipped, no traceback | REQ-8 |
| `compat --write` fails after `newest_tested` is set | exit 1, `compat.toml` and both copies return to their prior bytes, output contains `docs/compatibility.md` | REQ-6, test 8 |
| `bump` after the two constants read `CURRENT` | `pinned.rs` bytes unchanged, `PINNED` updated | REQ-3, test 7 |
| Nested `recorded_against`, or a non-json file | not printed | REQ-8, test 9 |
| Inline title collision | exit 1, output contains `a title collision in the generator`, bytes unchanged | REQ-5, test 3 |
| `gh` missing | exit 1, `gh is not installed`, none of the four case phrases | REQ-4, test 4 |
| Check names `events.rs` | output contains `a removed event still named in crates/hermes-contract/src/events.rs` | REQ-5 |
| Check path contains `/tests/` | output contains `a recorded frame or a mock that rejects a new field` | REQ-5 |
| Check contains `error[E0063]` and neither path above | output contains `a new field that a hand-written struct literal must name` | REQ-5 |
| None of those markers | the one-line error only | REQ-5 |

The floor cell is not in this table of writes. The floor is `v0.23.0`, the constant `pub const OLDEST_SUPPORTED: (u64, u64, u64)` with value `(0, 23, 0)`, provided by `PROOF-1`. That constant is not required to be on `main` before this issue starts. This issue does not read it and does not write it. Test 1 asserts the fixture floor is still `v0.23.0` after a successful bump. `tools/prepush.sh` is provided by `SEAT-0` and is on `main` before this issue starts. Run it as `tools/prepush.sh`.
