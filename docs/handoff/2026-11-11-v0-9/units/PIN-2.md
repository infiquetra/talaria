# PIN-2: Tests stop hard-coding which Hermes release is pinned

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

`crates/hermes-contract/PINNED` is two lines, `tag=` and `commit=`. `hermes_contract::pinned::TAG` and `COMMIT` are that file. `pin()` in `crates/talaria-core/tests/doctor_diagnose.rs:4` copies both into a `Pin`. `ready()` (`doctor_diagnose.rs:27`) still sets `code_version` to the quoted string `0.22.4` (`doctor_diagnose.rs:38`).

`version_verdicts` (`doctor_diagnose.rs:81`) already builds the short commit prefixes from `pin().commit`, and still quotes `0.22.4` as the version that should be `Pinned`. The rendered details at lines 139-140 quote `0.22.4` and `4d043038`. `capability_rows` (`doctor_diagnose.rs:167`) quotes the tag in the fix at line 188. `text_and_json_golden` (`doctor_diagnose.rs:602`) quotes the same tag and prefix at lines 611 and 613. REQ-2 is that set. The other versions in the table are not the pin.

`matrix_parses_and_every_row_is_complete` (`config_surface_matrix.rs:284`) reads `PINNED` and requires `hermes_release` to equal the `tag=` line. `results_parse_and_every_run_is_complete` (`gateway_concurrency_results.rs:166`) does the same through `pinned_tag()` (`gateway_concurrency_results.rs:634`). `validate_results` (`report.rs:681`) returns `Err("results identity")` when `hermes_release` is not `RELEASE` (`engine.rs:10`, which is `TAG`). `merge_raw_record` (`report.rs:612`) stores `hermes_release` and refuses a second release. That refusal is not a comparison with the pin. REQ-3 keeps the refusal and drops the comparison with `RELEASE`.

`known_but_unmodelled_events_are_silent` (`session_rules.rs:1187`) reads `x-notifications` from the vendored contract and asserts `names.len() >= 80` before the per-name allow check. REQ-4 deletes only that count.

`PROFILE_METHODS` is a hand-written slice (`profile_methods.rs:3`). `profile_injector` (`profile_methods.rs:246`) inserts `profile` when the method is in the slice. `profile_methods_match_vendored_contract` (`remote_connect.rs:427`) walks the vendored contract for a `profile` property. REQ-5 makes the generator emit the slice that walk already expects. `generate` (`contract_gen.py:506`) writes every file in its set (`contract_gen.py:549`), including `src/generated/mod.rs` from the string at `contract_gen.py:535`. The new module is added to that string. The commit contains `profile_methods.rs` and `mod.rs`. Any other generated file that differs from `main` is reported and not committed.

`final_version_requires_release_notes` (`release_metadata.rs:357`) requires the notes of a final workspace version to contain `pinned_tag()` (`release_metadata.rs:404`). The workspace version is `0.8.0` (`Cargo.toml:20`). `is_final` (`release_metadata.rs:638`) is true when the version has no `-` or `+`. The 0.8.0 notes already name Hermes v0.22.4. REQ-7 points that check at the `0.8.0` entry of `compat.toml` (`talaria`, `floor`, `pinned_tag`, `pinned_commit`, `newest_tested`) instead of at `PINNED`. `lib.rs` changes `mod generated` to `pub mod generated` so `hermes_contract::generated::profile_methods::PROFILE_METHODS` resolves, and `profile_methods.rs` re-exports that const. `GENERATED_FILES` in `tools/contract-gen/tests/support.py` gains `profile_methods.rs`.

## Tests

The issue states the two commits once. The first commit is only the new failing tests. `generated_profile_methods_equal_the_vendored_contract` is one of them, beside `profile_methods_match_vendored_contract` (`remote_connect.rs:427`). It reads `hermes_contract::generated::profile_methods::PROFILE_METHODS` and fails to compile until the generator writes that file. The second commit holds the other edits, including the pin literals, the REQ-3 renames and `tools/contract-gen/tests/support.py`. The newest `talaria` is the greatest of the three integer parts, compared from the left.
