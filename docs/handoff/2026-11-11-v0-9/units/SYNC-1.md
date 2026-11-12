# SYNC-1: Compatibility table: which Talaria release matches which Hermes releases

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document. Read the issue for REQ-1 through REQ-5 and the four tests. This page is the map of the pin, the version flag, and the generator those tests stand on. It does not restate the requirements.

## What exists today

`crates/hermes-contract/PINNED` is two lines, `tag=` and `commit=`. Today they are `tag=v0.22.4` and `commit=4d04303818b61bf3aa09278af48356315515965b`. `PIN-3` replaces those lines and adds the 0.9.0 row to `compat.toml` before this unit starts. `PIN-2` creates `compat.toml` and merges before `PIN-3`, so the file and the 0.9.0 row are both on `main` before this unit starts. This unit does not write that file. The row keys are `talaria`, `floor`, `pinned_tag`, `pinned_commit` and `newest_tested`. `crates/hermes-contract/src/pinned.rs` exposes `TAG` and `COMMIT`. `PROOF-1` (start) adds `pub const OLDEST_SUPPORTED: (u64, u64, u64)` with value `(0, 23, 0)` and is on `main` before this unit starts. `crates/talaria-core/src/doctor.rs` has `pub const MIN_HERMES: (u64, u64, u64) = (0, 22, 3)`. `SYNC-2` replaces that use.

`crates/talaria-cli/src/args.rs` `render_usage` prints one line for `DisplayVersion`. The command's `version` attribute sends both `--version` and `-V` to that one branch. `crates/talaria-cli/tests/cli.rs::installed_binary_reports_its_version` clears the environment, sets `PATH` to `/usr/bin:/bin`, keeps `LLVM_PROFILE_FILE`, and asserts that stdout is exactly `talaria {CARGO_PKG_VERSION}\n`. The workspace package version is `0.8.0` in `Cargo.toml`. This unit does not change that version. `hermes-contract` is a dev-dependency of `talaria-cli` today. The binary needs it as a normal dependency to read `CURRENT`.

`tools/contract-gen/contract_gen.py` `main` registers `generate`, `check`, `verify-vendor`, `pin` and `drift`. `pin` validates and prints the pin. It does not write a table. The file uses the standard library only. `compat` reads the table with `tomllib`. `check()` treats every `*.rs` file under `src/generated/` as typify output. `compat.rs` is excluded from that comparison. `generate()` does not write or delete it. A `compat.toml` with no `[[release]]` entry, or an entry missing a key, raises `ContractError` and the command exits 1.

`README.md` does not name a Hermes release. `## Install` is the first section heading. `docs/release-notes/v0.8.0.md` has a "Check your gateway" note that names v0.22.4. There is no `docs/release-notes/v0.9.0.md` and no `docs/compatibility.md`.

`.github/workflows/contract.yml` has two jobs. `contract-gen-check` runs on push and pull request, and only when a changed path is in `contract-paths`. That list does not include `README.md` or `docs/compatibility.md`. REQ-4 adds those two paths and the `compat --check` step to `contract-gen-check`. `contract-drift` stays as it is.

## Why the first row looks like the pin

The newest tested release is the newest Hermes the tests have passed against. When this pull request lands, that release is the pin `PIN-3` wrote, and it is the last `[[release]]` entry. `CURRENT` is that last entry. Its `talaria` is `0.9.0`. The package version on line 1 of `--version` stays `0.8.0`. The floor string on that row is `v0.23.0`, which is the same version as `OLDEST_SUPPORTED` `(0, 23, 0)` and not the older doctor tuple `(0, 22, 3)`. The notice in `SYNC-2` uses the floor and the newest tested value. The doctor's newer-than-pin verdict stays a comparison with the pin.

## Edge cases

| Case | Result |
| --- | --- |
| `CURRENT.pinned_tag` or `CURRENT.pinned_commit` differs from `PINNED` | `current_matches_pin_and_floor` fails |
| `CURRENT.floor`, after one leading `v` is stripped, is not the three numbers in `OLDEST_SUPPORTED` | the same test fails |
| README or `docs/compatibility.md` edited by hand | `compat --check` exits 1 and names the path |
| The file has no `[[release]]` entry | stop and report. Do not invent a row, and do not change the package version. |
| More than one `[[release]]` | `CURRENT` is the last one. Line 1 of `--version` stays the package version. |

`compat --write` writes `crates/hermes-contract/src/generated/compat.rs`. Consumers, including `--version` and `SYNC-2`, read `hermes_contract::compat::CURRENT`. Nothing parses `compat.toml` at run time.
