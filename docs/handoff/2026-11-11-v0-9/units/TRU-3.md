# TRU-3: Local /clear starts a fresh session in the same folder and profile

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue says what `/clear` does. This file is the map of the command table, the slash sort, and the session calls. It restates no requirement and no test. It refers to them by id.

## Why forwarding fails

`slash_line` (`crates/talaria-app/src/app.rs:9968`) looks up `LocalCommandTable::builtin` (`crates/talaria-core/src/commands/local.rs:559`). `clear` is not in `BUILTIN` (`local.rs:132`). The function then returns `Slash::Command` (`app.rs:9994`), and the submit match calls `run_command` (`app.rs:8474`).

Hermes v0.23.0, `hermes_cli/commands.py`, defines `CommandDef("clear", "Clear screen and start a new session", "Session", cli_only=True, desktop="terminal")`. `gateway/session_commands.py` `command_verdict` returns `refused` when the name is in none of the allow-sets in that file. `clear` is not in those sets. `REFUSAL_HINTS` in the same file has no `clear` key. The refusal is `unsupported_command`.

`unsupported` (`crates/talaria-core/src/commands/outcome.rs:188`) prints "/{name} is not available on the shared Hermes gateway." when the catalogue knows the name and there is no detail (`outcome.rs:217-224`). REQ-3 leaves this function as it is. A local `/clear` never reaches it.

## The local path

`LocalAction` is `non_exhaustive` (`local.rs:12-14`). `Quit` is the variant whose comment says the Hermes session keeps running (`local.rs:24-25`). The `quit` row is the pattern for a command with no aliases and `takes_argument: false` (`local.rs:133-140`).

`slash_line` maps `LocalAction::Quit` to `Slash::Quit` and every other action to `Slash::Local` (`app.rs:9982-9991`). An extra argument on a command that does not take one becomes `Slash::Notice` (`app.rs:9976-9980`). `clear` should use that existing notice, not a new one.

The submit match (`app.rs:8447-8474`) sends `Slash::Local` to `local_effect` (`app.rs:5779`). That function is a chain of `if action == ...` returns. `LocalAction::Clear` needs its own return before the fallthrough. `apply` (`app.rs:11546`) is not async. `send_later` (`app.rs:11014`) requires a `'static` future. `AppEffect::Switch` (`app.rs:12944`) solves that with `working_copy` (`app.rs:1466`) and `into_install` (`app.rs:1560`). REQ-2 uses that pair. `SessionCleared` is three outcomes, because `Option<SwitchInstall>` cannot tell a failed detach from a failed create, or the resume sentence from the session-gone sentence. `OpenFailed` carries `left`. `left` is true only after a live detach. The resume command in both notices that name one is the command inside `exit_hint` (`launch.rs:208-218`), including ` --profile <name>` when the profile is not `default`. When `left` is false the notice is `The previous session had ended, and a new one could not be started: <reason>.` and names no resume command. A failed detach says `You are still in it.` `Executor` (`app.rs:10881`) is private, so the session-gone case asserts `ClearSession { skip_detach: true }` from `submit` and does not watch the gateway.

## Session calls

`SessionService::detach` (`sessions.rs:410`) sends `session.detach` with the open session id and subscription id. `SessionService::open` (`sessions.rs:211`) takes `SessionRequest::New` and eventually calls the private `create` (`sessions.rs:297`), which sends `session.create`. `create_params` (`sessions.rs:480-489`) writes `request_id`, `source`, `cwd`, and only the model, provider, and reasoning values that were set. It does not write a profile. The profile stays the one on the service.

`NewSession.cwd` is `launch.rs:37`. The foreground folder on the app state is `state.folder`, a `SessionFolder` whose `cwd` is `Option<String>` (`crates/talaria-core/src/workspace.rs:117-119`). The launch folder is `state.launch_folder` (`app.rs:897`). REQ-2 says which one becomes `NewSession.cwd`.

`detach` returns `DetachOutcome::StaleSubscription` when the reply has `detached: false` (`sessions.rs:416-417`) and `DetachOutcome::Failed` for every `Err`, dropping the `CallError` (`sessions.rs:420`). The enum is `Copy` and `Failed` has no fields (`sessions.rs:132-136`). `CallError`'s `Display` for an RPC error is `RpcError`'s `Debug` (`rpc.rs:305-308`). `Behavior::Refuse` sends code 4001 (`mock_gateway.rs:49` and `3444`). REQ-2 changes only the `Failed` arm. `notice_lines` (`app.rs:2345`) is private. It prefixes a middle dot and a space (`app.rs:2346`). `committed_lines` (`session_flow.rs:219`) reads `AppEffect::CommitLines`. `opened_session` (`app.rs:9167`) commits its notice argument at `app.rs:9383`.

`stop_voice` (`switch.rs:382`) is what a switch calls before `detach`. REQ-2 calls it on the working copy. `HELD_PROMPTS_TEXT` (`switch.rs:30`) is the line a switch commits when `held` is not empty (`switch.rs:63`). `LINK_STOPPED_NOTICE` (`app.rs:152`) is private. Its text is `This prompt was not sent: the connection to Hermes has stopped.` `session_flow.rs:1197` sets `session_gone` and `session_flow.rs:1205` sets `link_failed` on an `AppState` that already has a session. REQ-2 uses those facts. The checks read the live state, because `working_copy` (`app.rs:1466`) copies neither `held`, `link_failed`, nor `session_gone`.

A running turn is `TurnState::Running` (`state.rs:20`), read with `turn()` (`state.rs:270`). `open_prompts` (`state.rs:300`) yields prompts in `Open` or `Answering`. REQ-2 uses that iterator and does not also test `state.masked`. `set_resume_snapshot` is `mock_gateway.rs:4962`. `running_turn` is `session_flow.rs:170`.

## Queue rows this pull request removes

`docs/engineering-journal/QUEUED.md:52` is the heading "Add a local Talaria `/clear` session-reset shortcut alongside Hermes `slash.exec`". `QUEUED.md:78` is "Implement `/clear` in the slash-command palette". REQ-5 removes those two entries only.

## Tests already in the tree

`local_table_names_are_unique` (`crates/talaria-core/tests/commands_local.rs:88`) walks `builtin()` and rejects a repeated name. `unsupported_with_a_held_catalogue` (`crates/talaria-core/tests/commands_outcome.rs:250`) locks the unknown-name sentence for `/nope`. `Rig` (`crates/talaria-app/tests/session_flow.rs:377`) is the gateway and folder the new session test uses. `Rig.folder` is the canonical folder (`session_flow.rs:388-392`).
