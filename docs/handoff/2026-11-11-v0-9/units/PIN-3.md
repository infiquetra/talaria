# PIN-3: Move the Hermes pin to v0.23.1

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`crates/hermes-contract/PINNED` is `tag=v0.22.4` and a 40-character commit. `pinned.rs` copies those into `TAG` and `COMMIT` (`pinned.rs:3-4`). `live_skip_reason` (`pinned.rs:7`) returns `None` only when `code_sha` is a prefix of `COMMIT`. REQ-1 changes the constants and does not edit that function.

`client_capabilities::Params` is built with only `server_requests: true` (`connect.rs:534-536`). `file_attach::Params` names `session_id`, `path`, `data_url`, `name` and `profile` (`attachments.rs:891-897`). `RollbackListParams` names `profile` and `session_id` (`rewind.rs:92-95`). `events.rs:189-190` maps `browser.controller.cancel` and `browser.controller.command`. REQ-2 is those four edits.

`generate` (`contract_gen.py:506`) sorts schema keys (`contract_gen.py:518`) and writes `src/generated/`. `FileAttachParams` lists `session_id` first in the vendor schema and `data_url` first in the generated struct (`schemas.rs:7537`). When `--hermes-checkout` is set, generate rewrites `tools/contract-gen/native-owner-methods.txt` (`contract_gen.py:551-552`). The checkout is outside this repository and its HEAD must match `PINNED`. Generate needs cargo-typify 0.8.0.

`client_shape` allows only `server_requests` on `client.capabilities` (`mock_gateway.rs:3013`). `caps_frame` (`connect.rs:2226`) expects params with `server_requests` only. `shared_names_have_distinct_parameter_types` (`methods.rs:87`) lists 18 names. `legacy_session_compress_is_not_a_native_method` (`native_method.rs:7`) compares stderr with `tests/compile_fail/legacy_session_compress.stderr`.

The eight `session_flow.rs` tests, the two `review_draft.rs` tests and the two `cli_process.rs` tests named in the issue open a session through that mock. `headless_stream.rs` `one_turn_record_sequence` (`headless_stream.rs:262`) and `two_turns_one_session_in_order` (`headless_stream.rs:402`) are the two print-mode stream tests decision 3 leaves untouched.

`compat.toml` is created by [#543](https://github.com/infiquetra/talaria/issues/543) (`PIN-2`) with the keys `talaria`, `floor`, `pinned_tag`, `pinned_commit` and `newest_tested`. REQ-4 adds the `0.9.0` row and does not edit release notes.

## Tests

Tests 1 and 3 fail on an assertion on today's crate. `caps_frame` expects `{"desktop_local":false,"server_requests":true}`. Test 2 fails to compile until `generate` creates the canonical modules. At v0.23.0 the three `groups.peer` names are not in `x-canonical-methods`; re-verify that at v0.23.1 before the test commit. The stderr is captured from an uncommitted generate and committed with the tests. The session, review and process tests named in the issue pass today and are not red. The mock allow-list moves in the first commit. The second commit vendors, generates and sets the fields, and does not edit those recordings again. The sha256 file is one digest, two spaces, the contract file name, and a newline. `final_version_requires_release_notes`, as [#543](https://github.com/infiquetra/talaria/issues/543) (`PIN-2`) leaves it, checks the 0.8.0 notes against the 0.8.0 `compat.toml` entry, not against `PINNED`. `newest_compat_entry_matches_pinned` is also [#543](https://github.com/infiquetra/talaria/issues/543) (`PIN-2`)'s test: the greatest `talaria` matches `PINNED`.
