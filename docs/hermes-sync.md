# Updating Hermes compatibility

Hermes has no release schedule. These commands take an explicit tag and set no
polling schedule. They use the last `[[release]]` entry in
`crates/hermes-contract/compat.toml`; the Talaria version and compatibility floor
remain unchanged.

```sh
python3 tools/contract-gen/contract_gen.py bump --tag <tag>
python3 tools/contract-gen/contract_gen.py tested --tag <tag>
```

Replace `<tag>` with a stable release tag such as `v<major>.<minor>.<patch>`.
For `bump`, install the generator backend described in
[the generator README](../tools/contract-gen/README.md) and authenticate `gh`
with read access to the public `NousResearch/hermes-agent` repository.

`bump` validates the local compatibility table before contacting GitHub. It
resolves the tag, reads and validates the contract, and fetches the licence.
It updates `PINNED`, the vendor bytes and checksum, and the last row's pin cells.
Newest-tested rises when the requested tag is newer and never falls. It generates
bindings without a Hermes checkout, invokes `compat --write` directly, then runs
exactly `cargo check --workspace --all-targets`.

The Rust pin constants read `hermes_contract::compat::CURRENT`; the command never
rewrites their source. On success it lists tracked JSON files under `crates/` and
`fixtures/` whose top-level string `recorded_against` is older than the new pin.
Nested keys, malformed files, and invalid or non-string releases are skipped.
It does not modify those recordings.

`tested` accepts only a stable tag newer than the last row's newest-tested value.
It updates that value and regenerates the compatibility copies without calling
GitHub or changing the pin or vendor files. An older or equal tag exits 1 with
`newest tested is already <current>`.

Both commands remember output bytes and path types before writing. A failed
update restores previous files, removes newly created outputs, and keeps
pre-existing directories, including those under the generated tree. A missing
release entry fails before a fetch. A missing `gh` prints `gh is not installed`;
a missing tag or fetch failure prints its existing error without a runbook phrase.
A failed `compat --write` prints that generator's message without a runbook phrase.

## Failures that need a reviewed fix

Validation, generation, or the workspace check prints a one-line error and the
first matching phrase below. Classification uses the full subprocess stderr,
even though the printed error retains only its last three lines. Each case
restores the command outputs and leaves the named hand-written files unchanged.

| Printed phrase | File left unchanged and remedy |
| --- | --- |
| `a title collision in the generator` | `tools/contract-gen/contract_gen.py` stays unchanged. Inspect the conflicting enum titles and ask the generator owner for a deliberate fix; the command does not add a `SCHEMA_REMOVALS` row. |
| `a removed event still named in crates/hermes-contract/src/events.rs` | `crates/hermes-contract/src/events.rs` stays unchanged. Compare the new notification catalogue with the hand-written event list and prepare a reviewed adapter fix. |
| `a recorded frame or a mock that rejects a new field` | The test or mock stays unchanged. Compare its rejected fields with the contract and recorded traffic, then prepare a reviewed recording or mock update without weakening assertions. |
| `a new field that a hand-written struct literal must name` | The hand-written Rust file stays unchanged. Read the new field's contract semantics and explicitly choose the value in a reviewed caller change. |

The matching order is enum title collision, `crates/hermes-contract/src/events.rs`,
a path containing `/tests/`, then `error[E0063]`. If none matches, only the error
is printed. Do not treat a failed update as a completed pin change.

## Safety rules

- The command and workflow read exactly two files from Hermes at the tag:
  `apps/shared/src/gateway-contract.openrpc.json` and `LICENSE`. They never
  execute code from that repository.
- The workflow uses only this repository's own token, with contents and issues
  write permission, and no other credentials. A person reviews the generated diff
  before any merge.

The commands use no secrets beyond the credentials `gh` already uses for
`verify-vendor`. Neither command merges, pushes, or creates a tag. Check the
resulting copies with `python3 tools/contract-gen/contract_gen.py compat --check`.
