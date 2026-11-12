# PROOF-1: Live tests run against any supported gateway and record its version

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read the issue for what to build, which tests to write, and when to stop. Use this document for the code that exists today, the Hermes evidence, and the comparison table. It does not restate a requirement or a test; it points at them by id.

## What the skip is today

The pin is tag `v0.22.4`, commit `4d04303818b61bf3aa09278af48356315515965b` (`crates/hermes-contract/src/pinned.rs:3-4`). `live_skip_reason` (`pinned.rs:7-23`) returns `None` only when `code_sha` is a prefix of that commit, at least 7 hex digits, compared case-insensitively. Any other sha, or a missing sha, is a skip. The mismatch sentence is `gateway code_sha "<sha>" does not match the pinned Hermes commit <commit> (<tag>); live tests are skipped`.

`live_preconditions` (`crates/hermes-client/tests/support/mod.rs:923-959`) discovers with `EnsurePolicy::Never`, resolves the literal `"fixture"` (`support/mod.rs:943`), then returns that skip (`support/mod.rs:952-954`). The same three steps are copied in `crates/talaria-app/tests/support/live.rs:67-104`. `report_skip` (`support/mod.rs:962-966`) prints `<prefix>: SKIPPED (<reason>)` and panics with `live gateway required` when `TALARIA_LIVE_REQUIRE` is exactly `1`.

`incompatible_live_revision` (`support/mod.rs:916`, and `live.rs:60`) calls `live_skip_reason` on a raw `identify` reply after `DiscoveryError::InvalidIdentity`. That probe reads `code_sha` only. REQ-5 keeps that call only when `TALARIA_LIVE_EXACT` is `1`.

The recorder's `preflight` rejects an empty sha, a sha shorter than seven characters, and any sha that is not a prefix of `PINNED` (`tools/record-fixture/src/lib.rs:401-415`). `pinned_sha_mismatch_or_missing_is_exit_2` (`scenarios.rs:820-838`) expects those three refusals. REQ-9 deletes that comparison. `preflight` takes `exact: bool` and does not read the environment. The two scenario tests pass `false`. With `code_version` `0.23.0` and `exact` false, the three cases are accepted. When `incompatible_live_revision` skips under `live_exact()`, only the commit is known: print `gateway_version_line` with an unknown version and the commit as received, then the existing reason. A profile is named in a request when a live test puts `fixture` in a call parameter, a `--profile` argument, a launch or manifest profile field, a fleet foreground profile, or `resolve_profile`. The file list includes every such file. Discovery's `Incompatible` path stays a panic. The `InvalidIdentity` panic is not a skip and does not print the version line. The feature argument is `Option<(&str, &str)>`, feature then since.

`Endpoint::code_sha` and `Endpoint::code_version` are `discovery.rs:78-82`. `from_identify` copies `result["code_sha"]` and `result["code_version"]` at `discovery.rs:215-219`.

## Cases the pinned tests call

`version_floor_skips_below_oldest` uses oldest `(0, 23, 0)` and `Some("0.22.4")`, `Some("unknown")`, `Some("  0.22.4")`, `Some("0.23")`, `None`, `Some("   ")`, `Some("0.23.0")`, `Some("0.23.1")`, `Some("v0.23.0")`, `Some("V0.23.0")`, and `Some("0.23.0+12.gabcdef")`. The older sentence quotes the original text, including the leading spaces of `  0.22.4`. Missing, `None`, and whitespace-only use the missing-version sentence. The five at-or-above inputs return `None`.

`feature_skip_names_the_feature` calls `feature_skip_reason(Some("0.23.0"), "sudo prompt", "0.24.0")` and expects `needs sudo prompt (Hermes 0.24.0 or later); this gateway runs 0.23.0`. The same feature and since with `Some("0.24.0")` and `Some("0.24.1")` return `None`. A missing version ends in `this gateway runs unknown`. Since `later` with version `0.23.0` returns `needs sudo prompt (Hermes later or later); this gateway runs 0.23.0`.

Read at Hermes v0.23.0. `gateway/status.py:812` returns `{"code_sha": identity.get("sha"), "code_version": identity.get("version")}`. `hermes_cli/version_info.py:346` sets `"version"` to `info.base_version`. `version_info.py:47-58` builds `display_version` as `base+distance` when the checkout is ahead of the tag. `identify` sends the base version, not the display version. REQ-1 still accepts a `+` suffix so a display string compares as its base triple.

`talaria_core::doctor::MIN_HERMES` is `(0, 22, 3)` (`crates/talaria-core/src/doctor.rs:8`). That constant is the doctor verdict, not `OLDEST_SUPPORTED`. This issue does not change it.

The example in `crates/hermes-client/examples/attach.rs:112-114` prints `gateway code_version={} code_sha={}` and uses the word `unknown` for an empty field (`attach.rs:129-134`). It also cuts the sha to 12 characters. REQ-4 trims each field first. Absent, empty, or spaces only prints `unknown`. Any other field prints the trimmed text, and the sha is not cut.

## Profile and smoke

`talaria --profile` defaults to `default` (`crates/talaria-cli/src/args.rs:50`). The smoke binary's `--profile` defaults to `fixture` (`tools/smoke/src/main.rs:48`). Phase 4a, 4b, and 5 refuse any other profile at `main.rs:110-124`. Phase 3 does the same at `main.rs:144-149`. REQ-7 resolves `TALARIA_LIVE_PROFILE` before those checks by the same trim rule as `live_profile`. Smoke does not depend on `hermes-contract`. The error text stays. `tools/smoke/tests/cli.rs:83-98` already reads the first `pane run` launch line, and `cli.rs:214-226` is the pattern of setting an environment variable on that command.

## Call sites that do not use the helper

These files call `live_skip_reason` themselves, or require the version string `0.22.4`. REQ-6 replaces that gate. It does not add a feature argument at the call.

- `crates/talaria-cli/tests/`: `live_session_picker.rs`, `workflow_live.rs`, `live_stream.rs`, `live_providers.rs`, `live_profiles.rs`, `live_model_roles.rs`, `live_mcp.rs`, `live_cron.rs`, `live_connectors.rs`, `live_configure.rs`, `live_bg.rs`, `live_attention.rs`, `live_attach_cmd.rs`, `live_background.rs`, `live_fleet.rs`, `live_rewind.rs`, `live_queue.rs`, `live_permissions.rs`, `live_pager.rs`, `live_automation.rs`, `live_attachments.rs`, `live_skin.rs`, `live_model.rs`, `live_comforts.rs`, `live_session.rs`
- `crates/talaria-app/tests/`: `live_review.rs`, `live_subagents.rs`, `live_fleet_view.rs`, `live_commands.rs`, `workflow_progress_live.rs` (`release_ok` at line 78 accepts only `0.22.4`)
- `crates/talaria-workflow/tests/support/live_port.rs:46-51` compares the sha with `PINNED`
- `crates/talaria-workflow/tests/support/progress_harness.rs:516` accepts only `0.22.4` or `v0.22.4`
- `crates/hermes-client/src/kanban.rs:379-402` (`live_compile_and_complete`) resolves `fixture` and passes `--profile fixture`. It does not check a version. REQ-6 replaces both literals with `live_profile()` and adds no floor gate. `const PROFILE: &str` in `progress_harness.rs:22`, `workflow_progress_live.rs:32`, `live_profiles.rs:14`, and `live_model.rs:15` feeds a request, so each becomes a function that returns `live_profile()`. A unit test that is not a live test, including `worktree_isolation.rs:367`, keeps the literal.

These live tests also name `fixture` in a request and are in the file list: `live_remote.rs`, `live_print.rs`, `live_side.rs`, `live_acp.rs`, `workflow_builtins_live.rs`, and `tools/record-fixture/tests/live_record.rs`. `live_skip_reason` has two message strings (`pinned.rs:18` and `:21`). The accepting arm returns `None` and has no message.

`crates/hermes-client/examples/call.rs:153` also calls `live_skip_reason`. It is not a test and is left unchanged. `connect.rs`, `record_hook.rs`, and `crates/talaria-app/tests/memory_service.rs` resolve `fixture` against an in-process mock. They are not this gate.

## Comparison

The parser is the one in REQ-1. `oldest` for the floor is `OLDEST_SUPPORTED`.

| Input | Against | `version_skip_reason` |
| --- | --- | --- |
| `0.22.4` | `(0, 23, 0)` | skip naming both versions |
| `0.23.0`, `v0.23.0`, `0.23.0+12.gabcdef`, `0.23.1` | `(0, 23, 0)` | `None` |
| missing or empty | `(0, 23, 0)` | the no-`code_version` sentence |
| `unknown` or `0.23` | `(0, 23, 0)` | skip naming the raw text |
| sha not the pin, `exact` false, version `0.23.1` | `live_test_skip` | `None` |
| same sha, `exact` true | `live_test_skip` | today's `live_skip_reason` sentence |

`feature_skip_reason` uses the sentence in REQ-3. Version `0.23.0` with since `0.24.0` skips. Version `0.24.0` does not. A missing version uses the word `unknown` in that same sentence.

`TALARIA_LIVE_EXACT` must be the character `1`. Any other value, including unset, is the floor. `TALARIA_LIVE_REQUIRE=1` is unchanged and applies to both modes.

## The identify reply carries the gateway version (added by the lead, 2026-11-11)

At Hermes v0.23.0 the `identify` answer is built by `build_identify_payload` (`gateway/control_socket.py:176-186`), which spreads in `_get_code_identity_fields()` (`gateway/status.py:799-812`): `code_sha` and `code_version`. So the probe that reads that reply can apply the floor as well as the exact-commit rule.
