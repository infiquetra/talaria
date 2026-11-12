# ACT-3: /copy puts an assistant reply on the clipboard

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read the issue for what to build. This file is the map of the code as it is today, with `path:line`, and the reason each settled decision fits that code. It does not restate a requirement or a test. REQ and AC numbers point at the issue.

## What exists today

A local command wins over a Hermes command of the same name and sends nothing (`crates/talaria-core/src/commands/local.rs:1-4`). `LocalAction` is the enum at `local.rs:14`. The builtin table has `export` at `local.rs:277-284` and has no `copy`. `LocalCommandTable::find` (`local.rs:572`) matches the name or an alias, with the slash optional and the case ignored. `parse_slash_line` (`crates/talaria-core/src/commands/parse.rs:38-55`) trims the argument.

`slash_line` (`crates/talaria-app/src/app.rs:9968`) keeps a name local only when that table has it. The `_` arm (`app.rs:9991`) returns `Slash::Local`. Any other name becomes `Slash::Command` (`app.rs:9994`), and that arm calls `run_command` (`app.rs:8474`). `Slash::Local` calls `local_effect` (`app.rs:8458-8460`). `local_effect` is at `app.rs:5779`. Its match at `app.rs:5937` sends `Export` into `model_command`, which calls `export_command` at `app.rs:7334`. A new variant that is not handled before that match falls through to `ContextRead` (`app.rs:5963`).

`local_action` (`crates/talaria-app/src/commands.rs:31`) maps `Quit` and `Agents` to key actions and returns `None` for everything else (`commands.rs:36`). That is why REQ-1 does not add an arm: an arm would be a key binding, which decision 7 refuses.

Assistant prose is `Cell::Assistant(AssistantCell)` (`crates/talaria-core/src/session/cell.rs:63-74`). The fields that matter are `text`, `sealed`, `interim` and `transformed`. `sealed` is what makes the cell final. `interim` means the cell came from `message.interim`. `transformed` means a hook replaced the reply. `SessionState::cells` returns every cell, oldest first (`crates/talaria-core/src/session/state.rs:198-199`). No numbered-reply selector exists. `insert_final` (`state.rs:243`) inserts at the final prefix. `advance_prefix` (`state.rs:410`) moves only over `Cell::is_final`, and an unsealed assistant cell is not final, so later final cells land in front of it. `SessionState::empty` is public (`state.rs:151`).

`strip_terminal_escapes` (`crates/talaria-core/src/session/text.rs:11`) removes CSI sequences and other escapes. Its comment says other callers treat a blank remainder as absent. REQ-4 does not. `assistant_reply` returns the stripped string, and the app copies that string. `CellId::new` is at `crates/talaria-core/src/session/ids.rs:35`.

`InlineTerminal::copy_to_clipboard` (`crates/talaria-tui/src/terminal.rs:724-728`) writes `osc52` (`crates/talaria-tui/src/providers/clipboard.rs:3`) and flushes. The `Result` is the write, not proof the terminal stored the text. `talaria-app` and `talaria-core` must not call it. `TuiScreen::copy_mcp_text` (`crates/talaria-cli/src/run.rs:3518-3519`) calls it and ignores the error. The `Screen` default is empty (`app.rs:516`). `AppEffect::CopyMemorySetup` calls `copy_provider_text` (`app.rs:11825`), which is a different path (`run.rs:3466`). Neither copies a conversation reply.

`notice_lines` (`app.rs:2345`) commits one line per input line, each prefixed with a middle dot and a space. The sentence `No session is open.` is already passed to it at `app.rs:5971` for compaction. `AppState.opened` is public (`app.rs:828`). `session_gone` is public (`app.rs:845`). `OpenedSession` and its `state` field are public (`crates/talaria-app/src/sessions.rs:34-35`).

The guide paragraph to extend is `docs/guide.md:48-50`, under `## Working in a session`. The key-doc tests that fail when a new key action is added are `doc_keys_equal_defaults` (`crates/talaria-tui/tests/keymap_doc.rs:60`) and `keybinding_tables_equal_default_key_map` (`crates/talaria-cli/tests/docs.rs:161`). The key enum is `Action` at `crates/talaria-tui/src/keys.rs:637`.

`lookup` (`local.rs:587`) and `parse` (`local.rs:599`) are what `local_line` (`app.rs:4743`) calls on a submitted slash line. `local_command_shadows_backend` is at `crates/talaria-core/tests/commands_local.rs:48`. `local_table_names_are_unique` is at `commands_local.rs:88`. `assistant_texts` is at `crates/talaria-core/tests/session_replay.rs:124`. `plain_answer_fixture_has_one_user_and_one_assistant_cell` is at `session_replay.rs:159`. `require_role` is at `session_replay.rs:46` and `reduce_log` at `session_replay.rs:80`. `OpenedSession`'s other fields are `stored_session_id`, `subscription_id`, `info`, `title`, `cursor`, `revision`, `execution_generation`, `launch_notices`, and `effort` (`sessions.rs:34-48`). The project check is `tools/prepush.sh`, on `main` before this issue starts.

## Why the decisions fit

Decision 4's second sentence skips an interim reply. `message.interim` sets `sealed` and `interim` together (`crates/talaria-core/src/session/reducer.rs:411-412`), so that reply is sealed while it is still the preview. Newest-first follows from `cells()` being oldest first: the last match is N = 1. Decision 5's five sentences are the whole notice set, because `copy_to_clipboard` cannot know whether the terminal stored the text. Decision 6 is the header at `local.rs:1-4`. Decision 7 is one sentence in the guide and no new `Action`, because the two key-doc tests compare the key table to the document.

This issue forwards nothing to Hermes. It marks no Hermes fact.

## Edges the tests lock

| Input | Result |
| --- | --- |
| empty argument | N = 1 |
| `"01"` and `"+1"` | N = 1. A leading plus is accepted. A positive whole number that does not fit in `u32` is not passed to `assistant_reply` |
| `"0"`, a fraction, or two tokens | the usage notice, and no copy |
| `link`, zero, a negative, or other text | the usage notice, before the session is considered |
| a too-large number with no session | `No session is open.` |
| a too-large number with a session and no such reply | `No reply` plus the text as typed |
| missing `opened`, or `session_gone` | `No session is open.`, and no copy |
| N past the kept cells | `No reply <N> to copy.`, and no copy |
| prepared length 100000 | copy |
| prepared length 100001 | the too-long notice, and no copy |
| sealed interim, or unsealed assistant | skipped |
| sealed transformed | kept, using `text` |

The usage check runs before the session check, so a bad token does not also say that no session is open. Exactly one notice is shown.
