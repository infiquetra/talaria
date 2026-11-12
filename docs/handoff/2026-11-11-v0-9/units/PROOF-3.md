# PROOF-3: Recorded sudo and secret prompts, and a helper that replays them

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read the issue for what to build, which tests to write, and when to stop. Use this document for the code and the Hermes evidence. It does not restate a requirement or a test; it points at them by id.

## What recording is today

`record-fixture record` writes JSONL under `fixtures/frames/` (`tools/record-fixture/src/main.rs:33-52`). On `main` today `--profile` defaults to the literal `fixture` (`main.rs:43-44`). `PROOF-1` changes the omitted flag to `live_profile()`. This issue starts after that merge and does not change the flag. `--scenario` repeats. An empty list records `SCENARIOS` only (`main.rs:142-146`). `Scenario::named` is defined in `lib.rs:115` and returns `None` for an unknown name. The error that lists `SCENARIOS`, `session-picker`, `FLEET_REGISTRY`, `FLEET_PEEK`, `SUBAGENT_CONTROL`, and `SCRIPTED_SCENARIOS` is in `main.rs:148-161`. `session-picker` is recorded by name and is not in `SCENARIOS` (`lib.rs:27-29`). REQ-1 follows that pattern for `sudo-prompt` and `secret-prompt`.

Every `Inbound::ServerRequest` in `wait_for_turns` (`lib.rs:2034-2038`) and in the subagent linger loop (`lib.rs:1986-1990`) is failed with `-32601`. REQ-2 answers method `sudo` and method `secret` with `{"value":""}` in both places. `check_evidence` (`lib.rs:518`) gains the two arms in REQ-3. `drive` (`lib.rs` near the `expected_statuses.len()` turn count) waits for one `message.complete` when that vector is empty for these two names.

`ReplaySource::open` reads a version-3 log (`crates/hermes-client/src/replay_source.rs:109`). `inbound()` yields frames whose `frame` is the JSON value (`replay_source.rs:203` and `:75-79`). REQ-4's helper returns the method and `params` of the inbound frame whose method is `sudo` or `secret`. The answer frame has no method. `redact.rs` withholds its whole `result` when the open request was `sudo` or `secret`, so the log does not store the empty value as a `password` field.

`NAMES` is `[&str; 29]` at `fixtures_replay.rs:20`. `listed_fixtures_exist` asserts the literal `29` at line 204. The directory count `33` is asserted at lines 118-119. Four files stay outside `NAMES`: `p5-04-model-options`, `p5-05-progress`, `p5-02-idempotency`, and `goal-completion-shared`. `fixture_texts` asserts the same directory count (`fixture_scan.rs:33-37`). `readme_inventory_matches_the_directory` compares the README links with the directory (`fixture_scan.rs:176-204`). The module doc that says `thirty-three` is line 5. `each_ends_with_expected_status` (`fixtures_replay.rs:277`) compares every file's `message.complete` statuses, and the header `expectedStatuses`, with `expected_statuses`. An unknown name falls through to `&["complete"]` (line 87). The recorder writes that header from `scenario.expected_statuses` (`main.rs:385`). `check_evidence` compares statuses at `lib.rs:531` before any later arm. REQ-2 returns before that comparison. REQ-1's test commit makes `each_ends_with_expected_status` require one `message.complete` and an empty header for these two names, and not a status word.

`FaultRelay::drop` and `FaultRelay::restore` are `fault_relay.rs:67-83`. REQ-6 leaves them alone.

## When Hermes asks

Read at Hermes v0.23.0.

Sudo. `tools/terminal_tool_sudo.py` rewrites a command that contains a real `sudo` only when a password is available. The sources, in order, are `SUDO_PASSWORD`, the session cache, then an interactive prompt. The prompt runs when `HERMES_INTERACTIVE` is set or a sudo callback is registered, the caller is not a delegated child or a single-query run, and `sudo -n true` is not successful. `gateway/session_sudo_prompt.py:19` sets `SUDO_TIMEOUT_S = 120`. `ask_sudo` (`session_sudo_prompt.py:22-26`) sends method `sudo` with params `{"command": command}`. `_ask` adds `session_id` (`gateway/session_server_requests.py:175-178`). An empty `value` is the skip (`terminal_tool_sudo.py` empty-password outcome). The command then runs unchanged and fails. The gateway installs that callback only for a local viewer turn.

Secret. `gateway/session_secret_prompt.py` sets `SECRET_TIMEOUT_S = 120` and sends method `secret` with `env_var`, `prompt`, and optional `metadata`. `_ask` adds `session_id`. `skill_view` (`tools/skills_tool.py`) asks when the skill front matter names a variable that is missing from the profile `.env` and the secret store. The callback is bound for a local viewer turn. An empty `value` is the skip: Hermes does not write `.env`, and the skill stays setup-needed. A non-empty value is saved to `.env`. This issue always sends the empty value.

`SudoRequestParams` is `command` and `session_id` (`crates/hermes-contract/src/generated/schemas.rs`). `SecretRequestParams` is `env_var`, `prompt`, `session_id`, and optional `metadata`. The method names are `sudo` and `secret` (`crates/hermes-contract/src/generated/server_requests.rs:98-128`).

## Scan

`record-fixture scan` runs `scan_for_secrets` and `scan_literals` on every JSONL file in the directory (`main.rs:478-544`). A live value under a suspicious key, including `password`, is rule `unredacted-suspicious-key` (`redact.rs:678-686`). Exit is 1 when any hit is printed. `only_allowed_profile_names` (`fixture_scan.rs:154`) allows only `fixture`, `default`, and `<profile>` in profile fields, and only `fixture` and `<profile>` in a `/profiles/` path segment.

## Edge cases

| Case | Result |
| --- | --- |
| `sudo -k true` and sudo asks | inbound method `sudo` with `session_id` and `command`. The script sends `{"value":""}`. A later `message.complete` is required (REQ-3). |
| sudo does not ask, or `SUDO_PASSWORD` is set | one line naming the precondition. No file is written (REQ-5). |
| `fixture-secret` is missing, or `FIXTURE_SECRET` is already in `.env` | one line naming the precondition. No file is written (REQ-5). |
| `skill_view` of `fixture-secret` and the variable is absent | inbound method `secret` with `session_id`, `env_var`, and `prompt`. The script sends `{"value":""}`. |
